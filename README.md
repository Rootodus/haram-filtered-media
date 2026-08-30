Haram-Filtered Media: Remove haram content from video and audio locally (text and online content are not yet supported).

- crates/hfm-core: a lib that is intended to be used by hfm-player (and potentially hfm-reader).
- crates/hfm-player: video player (plan is to also support YouTube videos and online videos).
- crates/hfm-reader (not available yet): text reader (plan is also to support website text).
- crates/hfm-web (potentially abandoned): was planned to be used for web content (video, audio, and text), but hfm-player and hfm-reader can replicate that, making this potentially not needed, also because this has complexity that may not be needed.

Question is whether hfm-player and hfm-reader should be separate, or be one single app. Perhaps they can first be separate for experimenting, and then finally be merged into one app.
