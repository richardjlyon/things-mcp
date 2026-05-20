//! AppleScript path for tag-admin operations (`create`/`rename`/`merge`/
//! `delete`/`move_under`). Things' JSON URL has no global tag-admin verbs,
//! so these go through `osascript -e <script>` and trust the synchronous
//! exit code as verification.
//!
//! Symmetric to `core/writer/` for the JSON URL path: a driver trait
//! (`AppleScriptDriver`) with a production impl (`OsascriptDriver`) and a
//! recording test impl (`RecordingAppleScript`), pure `render_*` helpers
//! in `script.rs`, and a facade (`TagAdmin`) in `admin.rs` that owns
//! the safety gate and result composition.

pub mod driver;
