//! Tag-aware MCP tool adapters. Two distinct flavours:
//!
//! - `things_list_tags` reads the SQLite reader pool and returns a flat
//!   list + a tag tree.
//! - `things_create_tag`, `things_rename_tag`, `things_merge_tags`,
//!   `things_delete_tag`, `things_move_tag` all route through the
//!   `TagAdmin` (`core/applescript/admin.rs`), which renders the
//!   appropriate AppleScript and hands it to `osascript`.
//!
//! `things_assign_tag` and `things_unassign_tag` live in `tools/todos.rs`
//! because they target a to-do row and run through the JSON URL chassis,
//! not AppleScript.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::core::applescript::admin::TagOutcome;
use crate::core::error::ThingsError;
use crate::core::reader::tags::{list_tags_with_tree, TagListing};
use crate::state::AppState;

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Default)]
pub struct ListTagsArgs {}

pub async fn things_list_tags(
    state: AppState,
    _args: ListTagsArgs,
) -> anyhow::Result<TagListing> {
    let listing = list_tags_with_tree(&state.pool).await?;
    Ok(listing)
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Default)]
pub struct CreateTagArgs {
    /// New tag name. Non-empty.
    pub name: String,
    /// Optional parent tag name. Omit for a root tag.
    #[serde(default)]
    pub parent: Option<String>,
}

pub async fn things_create_tag(
    state: AppState,
    args: CreateTagArgs,
) -> anyhow::Result<TagOutcome> {
    if args.name.trim().is_empty() {
        return Err(ThingsError::InvalidInput {
            field: "name".into(),
            reason: "name must be non-empty".into(),
        }
        .into());
    }
    if let Some(p) = args.parent.as_deref() {
        if p.trim().is_empty() {
            return Err(ThingsError::InvalidInput {
                field: "parent".into(),
                reason: "parent must be non-empty when supplied".into(),
            }
            .into());
        }
    }
    let out = state
        .tag_admin
        .create(&args.name, args.parent.as_deref())
        .await?;
    Ok(out)
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Default)]
pub struct RenameTagArgs {
    /// Current tag name.
    pub old: String,
    /// New tag name.
    pub new: String,
}

pub async fn things_rename_tag(
    state: AppState,
    args: RenameTagArgs,
) -> anyhow::Result<TagOutcome> {
    if args.old.trim().is_empty() {
        return Err(ThingsError::InvalidInput {
            field: "old".into(),
            reason: "old must be non-empty".into(),
        }
        .into());
    }
    if args.new.trim().is_empty() {
        return Err(ThingsError::InvalidInput {
            field: "new".into(),
            reason: "new must be non-empty".into(),
        }
        .into());
    }
    if args.old == args.new {
        return Err(ThingsError::InvalidInput {
            field: "new".into(),
            reason: "new must differ from old".into(),
        }
        .into());
    }
    let out = state.tag_admin.rename(&args.old, &args.new).await?;
    Ok(out)
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Default)]
pub struct MergeTagsArgs {
    /// Tag whose to-dos will be reassigned then deleted.
    pub source: String,
    /// Tag that absorbs the source tag's to-dos.
    pub target: String,
}

pub async fn things_merge_tags(
    state: AppState,
    args: MergeTagsArgs,
) -> anyhow::Result<TagOutcome> {
    if args.source.trim().is_empty() {
        return Err(ThingsError::InvalidInput {
            field: "source".into(),
            reason: "source must be non-empty".into(),
        }
        .into());
    }
    if args.target.trim().is_empty() {
        return Err(ThingsError::InvalidInput {
            field: "target".into(),
            reason: "target must be non-empty".into(),
        }
        .into());
    }
    if args.source == args.target {
        return Err(ThingsError::InvalidInput {
            field: "source".into(),
            reason: "source and target must differ".into(),
        }
        .into());
    }
    let out = state.tag_admin.merge(&args.source, &args.target).await?;
    Ok(out)
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Default)]
pub struct DeleteTagArgs {
    /// Tag name to delete. Removing a tag detaches it from every to-do that
    /// carries it; the to-dos themselves are unaffected.
    pub name: String,
}

pub async fn things_delete_tag(
    state: AppState,
    args: DeleteTagArgs,
) -> anyhow::Result<TagOutcome> {
    if args.name.trim().is_empty() {
        return Err(ThingsError::InvalidInput {
            field: "name".into(),
            reason: "name must be non-empty".into(),
        }
        .into());
    }
    let out = state.tag_admin.delete(&args.name).await?;
    Ok(out)
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Default)]
pub struct MoveTagArgs {
    /// Tag name to relocate in the tag tree.
    pub name: String,
    /// New parent tag name. Omit (or `null`) to promote to the root of the
    /// tag tree.
    #[serde(default)]
    pub new_parent: Option<String>,
}

pub async fn things_move_tag(
    state: AppState,
    args: MoveTagArgs,
) -> anyhow::Result<TagOutcome> {
    if args.name.trim().is_empty() {
        return Err(ThingsError::InvalidInput {
            field: "name".into(),
            reason: "name must be non-empty".into(),
        }
        .into());
    }
    if let Some(p) = args.new_parent.as_deref() {
        if p.trim().is_empty() {
            return Err(ThingsError::InvalidInput {
                field: "new_parent".into(),
                reason: "new_parent must be non-empty when supplied".into(),
            }
            .into());
        }
    }
    let out = state
        .tag_admin
        .move_under(&args.name, args.new_parent.as_deref())
        .await?;
    Ok(out)
}
