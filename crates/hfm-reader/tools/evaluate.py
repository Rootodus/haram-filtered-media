# /// script
# requires-python = ">=3.10"
# dependencies = [
#     "transformers>=4.40",
#     "optimum[onnxruntime]>=1.20",
#     "torch",
#     "sacrebleu",
#     "rouge-score",
#     "sentence-transformers",
#     "psutil",
# ]
#
# [[tool.uv.index]]
# name = "pytorch-cpu"
# url = "https://download.pytorch.org/whl/cpu"
# explicit = true
#
# [tool.uv.sources]
# torch = { index = "pytorch-cpu" }
# ///
"""Evaluate bart-base-detox (ONNX) on three corpora.

Usage:
    uv run tools/evaluate.py               # quality metrics only
    uv run tools/evaluate.py --mask        # test preemptive logit masking
    uv run tools/evaluate.py --latency     # + latency matrix
"""

import argparse
import csv
import random
import re
import sys
import time
import unicodedata
from collections import defaultdict
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

import psutil
import torch
from optimum.onnxruntime import ORTModelForSeq2SeqLM
from rouge_score import rouge_scorer
from sacrebleu.metrics import BLEU
from sentence_transformers import SentenceTransformer, util as st_util
from transformers import AutoTokenizer, LogitsProcessor, NoBadWordsLogitsProcessor, LogitsProcessorList

TOOLS = Path(__file__).parent
DATA = TOOLS / "data"
RESULTS = TOOLS / "results"
MODEL_ONNX = TOOLS.parent / "models" / "bart-base-detox" / "onnx"
PARADETOX_TSV = DATA / "paradetox" / "paradetox.tsv"
BANNED_WORDS = DATA / "banned_words.txt"
STRICT_BANNED = DATA / "strict_banned.txt"
RANT_FILE = DATA / "text_has_profanity.txt"

MAX_INPUT = 512
MAX_OUTPUT = 128
NUM_BEAMS = 5
EVAL_BATCH_SIZE = 16


# ---------- Text utilities ----------

def normalize(text: str) -> str:
    text = unicodedata.normalize("NFC", text).lower()
    text = re.sub(r"[\u200b-\u200f\u2060\ufeff]", "", text)
    return text


def whole_word_match(text: str, word: str) -> bool:
    if " " in word:
        return word in text
    return re.search(rf"(?<![a-zA-Z]){re.escape(word)}(?![a-zA-Z])", text) is not None


def banned_categories_in(text: str, entries: list[tuple[str, str]]) -> set[str]:
    norm = normalize(text)
    fired = set()
    for word, cat in entries:
        if whole_word_match(norm, normalize(word)):
            fired.add(cat)
    return fired


# ---------- Corpus loading ----------

def load_banned_words(path: Path) -> list[tuple[str, str]]:
    entries = []
    current = "uncategorized"
    for raw in path.read_text(encoding="utf-8").splitlines():
        line = raw.strip()
        if not line:
            continue
        if line.startswith("#"):
            current = line.lstrip("#").strip() or "uncategorized"
            continue
        entries.append((line, current))
    if not entries:
        sys.exit(f"No entries found in {path}")
    return entries


def load_paradetox(path: Path) -> list[dict]:
    rows = []
    with open(path, encoding="utf-8") as f:
        reader = csv.reader(f, delimiter="\t")
        next(reader)
        for row in reader:
            if not row:
                continue
            toxic = row[0].strip()
            neutrals = [n.strip() for n in row[1:] if n.strip()]
            if toxic:
                rows.append({"toxic": toxic, "neutrals": neutrals})
    return rows


def derive_strict_banned(rows, entries, out_path: Path, per_word: int = 2, cap: int = 100) -> list[str]:
    by_word = defaultdict(list)
    for row in rows:
        norm = normalize(row["toxic"])
        for word, _ in entries:
            if whole_word_match(norm, normalize(word)):
                by_word[word].append(row["toxic"])
    selected, seen = [], set()
    for word, sents in by_word.items():
        for s in sents[:per_word]:
            if s not in seen:
                seen.add(s)
                selected.append(s)
    if len(selected) > cap:
        random.seed(0)
        selected = random.sample(selected, cap)
    out_path.write_text("\n".join(selected) + "\n", encoding="utf-8")
    return selected


def load_rant_blocks(path: Path) -> list[str]:
    content = path.read_text(encoding="utf-8")
    blocks = re.findall(r"<p>(.*?)</p>", content, re.DOTALL)
    return [b.strip() for b in blocks if b.strip()]


# ---------- Masking ----------

class ArgmaxMaskedCounter(LogitsProcessor):
    def __init__(self, masked_ids: set[int]):
        self.masked_ids = masked_ids
        self.masked_count = 0
        self.total_count = 0

    def __call__(self, input_ids, scores):
        for a in scores.argmax(dim=-1).tolist():
            self.total_count += 1
            if a in self.masked_ids:
                self.masked_count += 1
        return scores


def build_masked_token_ids(tokenizer, entries) -> set[int]:
    masked = set()
    norm_words = [normalize(w) for w, _ in entries]
    for tid in range(len(tokenizer)):
        try:
            decoded = tokenizer.decode([tid], skip_special_tokens=False)
        except Exception:
            continue
        norm = normalize(decoded).strip()
        if not norm:
            continue
        for w in norm_words:
            if whole_word_match(norm, w):
                masked.add(tid)
                break
    return masked

def build_bad_words_ids(tokenizer, entries) -> list[list[int]]:
    """Generates token sequences for all variations of the banned words."""
    bad_words = []
    for word, _ in entries:
        variants = [
            word,
            f" {word}",
            word.capitalize(),
            f" {word.capitalize()}",
            word.upper(),
            f" {word.upper()}"
        ]
        for var in variants:
            # Tokenize without special tokens, just the raw text
            ids = tokenizer.encode(var, add_special_tokens=False)
            if ids and ids not in bad_words:
                bad_words.append(ids)
    return bad_words


# ---------- Generation ----------

def generate_batch(model, tokenizer, texts, num_beams=NUM_BEAMS, logits_processor=None):
    inputs = tokenizer(
        texts, return_tensors="pt", padding=True, truncation=True, max_length=MAX_INPUT
    )
    kwargs = dict(max_new_tokens=MAX_OUTPUT, num_beams=num_beams, early_stopping=True)
    
    if logits_processor is not None:
        kwargs["logits_processor"] = logits_processor
        
    t0 = time.perf_counter()
    with torch.no_grad():
        outputs = model.generate(**inputs, **kwargs)
    elapsed = time.perf_counter() - t0
    return tokenizer.batch_decode(outputs, skip_special_tokens=True), elapsed


# ---------- Metrics (same as before) ----------
def score_bleu(predictions, references):
    bleu = BLEU(effective_order=True)
    per_row = []
    for pred, refs in zip(predictions, references):
        if not refs:
            per_row.append(None)
            continue
        per_row.append(max(bleu.sentence_score(pred, [r]).score for r in refs) / 100.0)
    valid = [s for s in per_row if s is not None]
    return sum(valid) / len(valid) if valid else 0.0

def score_rouge(predictions, references):
    scorer = rouge_scorer.RougeScorer(["rougeL"], use_stemmer=True)
    per_row = []
    for pred, refs in zip(predictions, references):
        if not refs:
            per_row.append(None)
            continue
        per_row.append(max(scorer.score(r, pred)["rougeL"].fmeasure for r in refs))
    valid = [s for s in per_row if s is not None]
    return sum(valid) / len(valid) if valid else 0.0

def score_cosine(embedder, predictions, references):
    emb_preds = embedder.encode(predictions, convert_to_tensor=True)
    flat_refs = []
    ref_counts = []
    for refs in references:
        flat_refs.extend(refs)
        ref_counts.append(len(refs))
    emb_refs_all = embedder.encode(flat_refs, convert_to_tensor=True)
    per_row = []
    ref_idx = 0
    for i, count in enumerate(ref_counts):
        if count == 0:
            per_row.append(None)
            continue
        emb_refs = emb_refs_all[ref_idx:ref_idx+count]
        sim = float(st_util.cos_sim(emb_preds[i], emb_refs).max())
        per_row.append(sim)
        ref_idx += count
    valid = [s for s in per_row if s is not None]
    return sum(valid) / len(valid) if valid else 0.0


# ---------- Per-corpus run ----------

def run_corpus(name, texts, model, tokenizer, entries, embedder, logits_proc, references=None):
    print(f"\n=== {name} ({len(texts)} segments) ===", flush=True)
    
    outputs = []
    total_elapsed = 0.0
    for i in range(0, len(texts), EVAL_BATCH_SIZE):
        chunk = texts[i:i+EVAL_BATCH_SIZE]
        chunk_out, elapsed = generate_batch(model, tokenizer, chunk, logits_processor=logits_proc)
        outputs.extend(chunk_out)
        total_elapsed += elapsed
        print(f"  Processed {min(i+EVAL_BATCH_SIZE, len(texts))}/{len(texts)}...", flush=True)
        
    n = len(texts)
    cat_hits = defaultdict(int)
    any_hit = 0
    for out in outputs:
        cats = banned_categories_in(out, entries)
        if cats:
            any_hit += 1
        for c in cats:
            cat_hits[c] += 1
            
    all_cats = sorted(set(c for _, c in entries))
    per_cat = {c: cat_hits.get(c, 0) / n for c in all_cats}
    overall = any_hit / n
    bleu = rouge = cos = None
    if references is not None:
        bleu = score_bleu(outputs, references)
        rouge = score_rouge(outputs, references)
        cos = score_cosine(embedder, outputs, references)
        
    return {
        "name": name,
        "n": n,
        "overall_banned": overall,
        "per_cat": per_cat,
        "bleu": bleu,
        "rouge": rouge,
        "cosine": cos,
        "total_time": total_elapsed,
        "outputs": outputs,
    }


# ---------- Reporting ----------
def print_quality_table(reports):
    cats = sorted(set(c for r in reports for c in r["per_cat"]))
    header = (f"{'corpus':<22} {'n':>4} {'overall':>8}"
              + "".join(f" {c:>12}" for c in cats)
              + f" {'BLEU':>6} {'ROUGE':>6} {'cos':>6} {'avg_ms':>8}")
    print(header)
    print("-" * len(header))
    for r in reports:
        row = f"{r['name']:<22} {r['n']:>4} {r['overall_banned'] * 100:>7.1f}%"
        for c in cats:
            row += f" {r['per_cat'].get(c, 0) * 100:>11.1f}%"
        bleu = f"{r['bleu']:.3f}" if r["bleu"] is not None else "—"
        rouge = f"{r['rouge']:.3f}" if r["rouge"] is not None else "—"
        cos = f"{r['cosine']:.3f}" if r["cosine"] is not None else "—"
        avg_ms = (r["total_time"] / r["n"]) * 1000
        row += f" {bleu:>6} {rouge:>6} {cos:>6} {avg_ms:>7.1f}"
        print(row)


# ---------- Main ----------
def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--limit", type=int, default=None, help="limit rows per corpus")
    ap.add_argument("--mask", action="store_true", help="use NoBadWordsLogitsProcessor")
    args = ap.parse_args()

    for p in (PARADETOX_TSV, BANNED_WORDS, RANT_FILE):
        if not p.exists():
            sys.exit(f"Missing {p}.")

    RESULTS.mkdir(exist_ok=True)

    print("Loading banned words...", flush=True)
    entries = load_banned_words(BANNED_WORDS)
    
    print("Loading ParaDetox...", flush=True)
    para_rows_full = load_paradetox(PARADETOX_TSV)

    if not STRICT_BANNED.exists():
        strict_texts_full = derive_strict_banned(para_rows_full, entries, STRICT_BANNED)
    else:
        strict_texts_full = [l for l in STRICT_BANNED.read_text(encoding="utf-8").splitlines() if l.strip()]

    rant_texts_full = load_rant_blocks(RANT_FILE)

    para_rows = para_rows_full[: args.limit] if args.limit else para_rows_full
    strict_texts = strict_texts_full[: args.limit] if args.limit else strict_texts_full
    rant_texts = rant_texts_full[: args.limit] if args.limit else rant_texts_full

    print("Loading model (ORT)...", flush=True)
    tokenizer = AutoTokenizer.from_pretrained(str(MODEL_ONNX))
    model = ORTModelForSeq2SeqLM.from_pretrained(str(MODEL_ONNX))

    print("Loading sentence embedder...", flush=True)
    embedder = SentenceTransformer("all-MiniLM-L6-v2")

    # Setup Logits Processor
    logits_proc = None
    if args.mask:
        print("Building NoBadWordsLogitsProcessor...", flush=True)
        bad_words_ids = build_bad_words_ids(tokenizer, entries)
        print(f"  Built {len(bad_words_ids)} bad word token sequences.")
        logits_proc = LogitsProcessorList([
            NoBadWordsLogitsProcessor(bad_words_ids, eos_token_id=tokenizer.eos_token_id)
        ])

    reports = [
        run_corpus("paradetox", [r["toxic"] for r in para_rows],
                   model, tokenizer, entries, embedder, logits_proc,
                   references=[r["neutrals"] for r in para_rows]),
        run_corpus("strict_banned", strict_texts,
                   model, tokenizer, entries, embedder, logits_proc),
        run_corpus("text_has_profanity", rant_texts,
                   model, tokenizer, entries, embedder, logits_proc),
    ]

    print("\n" + "=" * 60)
    print("QUALITY")
    print("=" * 60)
    print_quality_table(reports)

    csv_path = RESULTS / ("summary_masked.csv" if args.mask else "summary_unmasked.csv")
    with open(csv_path, "w", newline="", encoding="utf-8") as f:
        w = csv.writer(f)
        all_cats = sorted(set(c for r in reports for c in r["per_cat"]))
        w.writerow(["corpus", "n", "overall_banned"] + [f"banned_{c}" for c in all_cats]
                   + ["bleu", "rouge", "cosine", "avg_ms"])
        for r in reports:
            w.writerow([r["name"], r["n"], f"{r['overall_banned']:.4f}"]
                       + [f"{r['per_cat'].get(c, 0):.4f}" for c in all_cats]
                       + [f"{r['bleu']:.4f}" if r["bleu"] is not None else "",
                          f"{r['rouge']:.4f}" if r["rouge"] is not None else "",
                          f"{r['cosine']:.4f}" if r["cosine"] is not None else "",
                          f"{(r['total_time'] / r['n']) * 1000:.1f}"])
    print(f"\nWrote {csv_path}")

if __name__ == "__main__":
    main()
