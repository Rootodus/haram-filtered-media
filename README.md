Haram-Filtered Media: Remove haram content from video and audio locally (text and online content are not yet supported).

- crates/hfm-core: a lib that is intended to be used by hfm-player (and potentially hfm-reader).
- crates/hfm-player: video player (plan is to also support YouTube videos and online videos).
- crates/hfm-reader (not done yet): text reader (plan may be also to support website text).
- crates/hfm-web (potentially abandoned): was planned to be used for web content (video, audio, and text), but hfm-player and hfm-reader can replicate that, making this potentially not needed, also because this has complexity that may not be needed.

Question is whether hfm-player and hfm-reader should be separate, or be one single app. Perhaps it is best if they are separate.

## License
Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.

### Contribution
Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in this project by you, as defined in the Apache-2.0 license,
shall be dual licensed as above, without any additional terms or conditions.
