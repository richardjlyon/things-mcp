//! Pure AppleScript render functions for tag-admin ops. No I/O — each
//! function takes the inputs the tool surface accepts and returns the
//! AppleScript source as a `String`. The driver (`OsascriptDriver`) and
//! the facade (`TagAdmin`) are the layers that actually run the script.

/// Escape a user-supplied string for safe inclusion inside an AppleScript
/// double-quoted literal. AppleScript's escape rules: backslash escapes
/// itself and a literal double quote.
pub fn escape_applescript_string(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

pub fn render_create_tag(name: &str, parent: Option<&str>) -> String {
    let name_q = escape_applescript_string(name);
    match parent {
        Some(p) => {
            let p_q = escape_applescript_string(p);
            format!(
                r#"tell application "Things3"
    set newTag to make new tag with properties {{name:"{name_q}"}}
    set parent tag of newTag to tag "{p_q}"
end tell"#,
            )
        }
        None => format!(
            r#"tell application "Things3"
    make new tag with properties {{name:"{name_q}"}}
end tell"#,
        ),
    }
}

pub fn render_rename_tag(old: &str, new: &str) -> String {
    let old_q = escape_applescript_string(old);
    let new_q = escape_applescript_string(new);
    format!(
        r#"tell application "Things3"
    set name of tag "{old_q}" to "{new_q}"
end tell"#,
    )
}

/// Reassign every to-do that carries `source` to also carry `target`, then
/// delete the `source` tag. AppleScript surface: `to dos of tag "source"`
/// enumerates the tasks; we add the target tag to each and then remove the
/// source tag from the global tag list.
pub fn render_merge_tags(source: &str, target: &str) -> String {
    let s_q = escape_applescript_string(source);
    let t_q = escape_applescript_string(target);
    format!(
        r#"tell application "Things3"
    set sourceTag to tag "{s_q}"
    set targetTag to tag "{t_q}"
    repeat with t in (to dos of sourceTag)
        set tag names of t to (tag names of t) & "{t_q}"
    end repeat
    delete sourceTag
end tell"#,
    )
}

pub fn render_delete_tag(name: &str) -> String {
    let name_q = escape_applescript_string(name);
    format!(
        r#"tell application "Things3"
    delete tag "{name_q}"
end tell"#,
    )
}

pub fn render_move_tag(name: &str, new_parent: Option<&str>) -> String {
    let name_q = escape_applescript_string(name);
    match new_parent {
        Some(p) => {
            let p_q = escape_applescript_string(p);
            format!(
                r#"tell application "Things3"
    set parent tag of tag "{name_q}" to tag "{p_q}"
end tell"#,
            )
        }
        // `missing value` is AppleScript's null; assigning it promotes the
        // tag to the root of the tag tree.
        None => format!(
            r#"tell application "Things3"
    set parent tag of tag "{name_q}" to missing value
end tell"#,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_handles_quotes_and_backslashes() {
        assert_eq!(escape_applescript_string("plain"), "plain");
        assert_eq!(escape_applescript_string("he said \"hi\""), "he said \\\"hi\\\"");
        assert_eq!(escape_applescript_string("path\\to"), "path\\\\to");
    }

    #[test]
    fn create_tag_no_parent_renders_make_tag() {
        let s = render_create_tag("Work", None);
        assert!(s.contains("tell application \"Things3\""));
        assert!(s.contains("make new tag with properties {name:\"Work\"}"));
        assert!(!s.contains("parent tag"));
    }

    #[test]
    fn create_tag_with_parent_renders_parent_link() {
        let s = render_create_tag("Urgent", Some("Work"));
        assert!(s.contains("make new tag with properties {name:\"Urgent\"}"));
        assert!(s.contains("set parent tag of newTag to tag \"Work\""));
    }

    #[test]
    fn create_tag_escapes_quotes_in_name() {
        let s = render_create_tag("She said \"yes\"", None);
        assert!(s.contains("name:\"She said \\\"yes\\\"\""));
    }

    #[test]
    fn rename_tag_renders_set_name() {
        let s = render_rename_tag("Old", "New");
        assert!(s.contains("set name of tag \"Old\" to \"New\""));
    }

    #[test]
    fn rename_tag_escapes_quotes() {
        let s = render_rename_tag("a\"b", "c\"d");
        assert!(s.contains("set name of tag \"a\\\"b\" to \"c\\\"d\""));
    }

    #[test]
    fn merge_tags_renders_loop_and_delete() {
        let s = render_merge_tags("Source", "Target");
        assert!(s.contains("set sourceTag to tag \"Source\""));
        assert!(s.contains("set targetTag to tag \"Target\""));
        assert!(s.contains("repeat with t in (to dos of sourceTag)"));
        assert!(s.contains("delete sourceTag"));
    }

    #[test]
    fn merge_tags_escapes_quotes_in_target_inside_loop_body() {
        let s = render_merge_tags("A", "B \"quoted\"");
        // The loop body assigns the target name via concatenation; the
        // escaped form must appear in both the binding line and the loop.
        assert!(s.contains("set targetTag to tag \"B \\\"quoted\\\"\""));
        assert!(s.contains("& \"B \\\"quoted\\\"\""));
    }

    #[test]
    fn delete_tag_renders_delete() {
        let s = render_delete_tag("Stale");
        assert!(s.contains("delete tag \"Stale\""));
    }

    #[test]
    fn delete_tag_escapes_quotes() {
        let s = render_delete_tag("Bad\"name");
        assert!(s.contains("delete tag \"Bad\\\"name\""));
    }

    #[test]
    fn move_tag_under_parent_renders_set_parent() {
        let s = render_move_tag("Urgent", Some("Work"));
        assert!(s.contains("set parent tag of tag \"Urgent\" to tag \"Work\""));
    }

    #[test]
    fn move_tag_to_root_uses_missing_value() {
        let s = render_move_tag("Urgent", None);
        assert!(s.contains("set parent tag of tag \"Urgent\" to missing value"));
    }

    #[test]
    fn move_tag_escapes_quotes_in_both_names() {
        let s = render_move_tag("a\"b", Some("c\"d"));
        assert!(s.contains("set parent tag of tag \"a\\\"b\" to tag \"c\\\"d\""));
    }
}
