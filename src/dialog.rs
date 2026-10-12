//! The reusable modal dialog (M2-W6 / T2).
//!
//! One widget covers every modal the reference builds with `web/dialog.ts`:
//! a confirmation ("Yes"/"No"), a message ("OK"), a single text prompt
//! (`showRefInput`) and a multi-input form (`showForm`), whose inputs may be
//! text fields, dropdowns (`Select`) or checkboxes.
//!
//! Reference anchors (read-only `../vscode-git-graph`):
//!
//! * `web/dialog.ts:199-296` — `showForm` builds one `<tr>` per input and gives
//!   each input a generated `tabindex` (`dialog.ts:216`, `288`), which is the
//!   focus order this module reproduces with a single `focus` index.
//! * `web/main.ts:2032-2040` — **C02/C12**: `Escape` closes the dialog and
//!   `Enter` submits it, from anywhere including inside an input (the reference
//!   keeps the `keyCode 13` check for CJK IME keyboards).
//! * `web/dialog.ts:393-400` — `submit()` runs the action, not the cancel path.
//! * `web/dialog.ts:94` — `WHITESPACE_REGEXP`: a ref name may not contain
//!   whitespace. [`Field::no_whitespace`] carries that rule.
//!
//! ## The 2-step safety rule (W5 → W6)
//!
//! W5's [`crate::actions::PlanKind::Confirm`] plans are built but deliberately
//! **not run** (`src/actions.rs:1252`). W6 keeps that two-step shape: the menu
//! builds the plan, this dialog asks the user, and only an explicit submit calls
//! [`crate::actions::execute_confirmed`]. Nothing in this module runs git on its
//! own, so a cancelled dialog is provably a no-op.

use crate::actions::{self, ActionPlan, ActionTarget, PushMode, RefAction, ResetMode};

/// One input row of a dialog form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field {
    /// Machine name used by [`Dialog::apply`] to write the value back into the
    /// [`ActionTarget`] (`"name"`, `"url"`, `"ref_name"`, `"reset_mode"`, …).
    pub name: String,
    /// Label shown to the left of the control.
    pub label: String,
    pub kind: FieldKind,
    /// The value as the user typed it (text fields only).
    pub value: String,
    /// Cursor position inside `value`, in characters.
    pub cursor: usize,
    /// Submitting with an empty value is refused (`dialog.ts:393-400`).
    pub required: bool,
    /// Refuse whitespace (`WHITESPACE_REGEXP`, `dialog.ts:94`).
    pub no_whitespace: bool,
}

/// What kind of control a [`Field`] renders.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldKind {
    /// A text input, optionally with a placeholder (`dialog.ts:216`).
    Text { placeholder: String },
    /// A `<select>` dropdown (`DialogInputType.Select`).
    Select {
        options: Vec<String>,
        selected: usize,
    },
    /// A checkbox (`DialogInputType.Checkbox`).
    Checkbox { checked: bool },
}

impl Field {
    /// A required text input.
    pub fn text(name: &str, label: &str, value: &str) -> Self {
        Field {
            name: name.to_string(),
            label: label.to_string(),
            kind: FieldKind::Text {
                placeholder: String::new(),
            },
            value: value.to_string(),
            cursor: value.chars().count(),
            required: true,
            no_whitespace: false,
        }
    }

    /// A text input that may be left empty.
    pub fn optional_text(name: &str, label: &str, value: &str) -> Self {
        Field {
            required: false,
            ..Field::text(name, label, value)
        }
    }

    /// A text input that refuses whitespace (`showRefInput`, `dialog.ts:137-150`).
    pub fn ref_text(name: &str, label: &str, value: &str) -> Self {
        Field {
            no_whitespace: true,
            ..Field::text(name, label, value)
        }
    }

    /// A dropdown.
    pub fn select(name: &str, label: &str, options: &[&str], selected: usize) -> Self {
        Field {
            name: name.to_string(),
            label: label.to_string(),
            kind: FieldKind::Select {
                options: options.iter().map(|option| (*option).to_string()).collect(),
                selected: selected.min(options.len().saturating_sub(1)),
            },
            value: String::new(),
            cursor: 0,
            required: false,
            no_whitespace: false,
        }
    }

    /// A checkbox.
    pub fn checkbox(name: &str, label: &str, checked: bool) -> Self {
        Field {
            name: name.to_string(),
            label: label.to_string(),
            kind: FieldKind::Checkbox { checked },
            value: String::new(),
            cursor: 0,
            required: false,
            no_whitespace: false,
        }
    }

    /// The text the row renders for this control.
    pub fn display_value(&self) -> String {
        match &self.kind {
            FieldKind::Text { .. } => self.value.clone(),
            FieldKind::Select { options, selected } => options
                .get(*selected)
                .cloned()
                .unwrap_or_else(|| "(none)".to_string()),
            FieldKind::Checkbox { checked } => {
                if *checked {
                    "[x]".to_string()
                } else {
                    "[ ]".to_string()
                }
            }
        }
    }

    /// Is this field valid on its own?
    fn validate(&self) -> Result<(), String> {
        match &self.kind {
            FieldKind::Text { .. } => {
                if self.required && self.value.trim().is_empty() {
                    return Err(format!("{} cannot be empty", self.label));
                }
                if self.no_whitespace && self.value.chars().any(char::is_whitespace) {
                    return Err(format!("{} cannot contain whitespace", self.label));
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }

    /// Insert `c` at the cursor (text fields only).
    fn insert(&mut self, c: char) {
        if !matches!(self.kind, FieldKind::Text { .. }) {
            return;
        }
        let byte = char_byte_index(&self.value, self.cursor);
        self.value.insert(byte, c);
        self.cursor += 1;
    }

    /// Delete the character before the cursor (text fields only).
    fn backspace(&mut self) {
        if self.cursor == 0 || !matches!(self.kind, FieldKind::Text { .. }) {
            return;
        }
        let byte = char_byte_index(&self.value, self.cursor - 1);
        self.value.remove(byte);
        self.cursor -= 1;
    }

    /// Delete the character under the cursor (text fields only).
    fn delete(&mut self) {
        if !matches!(self.kind, FieldKind::Text { .. }) {
            return;
        }
        if self.cursor >= self.value.chars().count() {
            return;
        }
        let byte = char_byte_index(&self.value, self.cursor);
        self.value.remove(byte);
    }
}

/// Byte offset of character index `chars` in `text`.
fn char_byte_index(text: &str, chars: usize) -> usize {
    text.char_indices()
        .nth(chars)
        .map(|(byte, _)| byte)
        .unwrap_or(text.len())
}

/// The action a dialog will run once the user submits it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingPlan {
    pub action: RefAction,
    /// The target as the menu built it; [`Dialog::apply`] layers the form
    /// values on top before re-planning.
    pub target: ActionTarget,
    /// The plan the menu built, shown in the dialog body so the user can see
    /// exactly which `git` argv they are approving.
    pub plan: ActionPlan,
}

/// A value a dialog form produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DialogValue {
    Text(String),
    Select(String),
    Bool(bool),
}

/// What [`Dialog::handle`] decided.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DialogResult {
    /// The user submitted; `button` is the index into [`Dialog::buttons`].
    Submit {
        values: Vec<DialogValue>,
        button: usize,
    },
    /// The user pressed `Escape` (`dialog.ts:369-380`).
    Cancel,
}

/// A keypress translated for the dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialogKey {
    Char(char),
    Backspace,
    Delete,
    Left,
    Right,
    Home,
    End,
    /// `Tab` / `Down` — next focusable (the generated `tabindex` order).
    Next,
    /// `Shift+Tab` / `Up` — previous focusable.
    Prev,
    /// `Enter` — submit (C02/C12).
    Submit,
    /// `Escape` — close.
    Escape,
    /// `Space` — toggle a checkbox.
    Space,
}

/// A modal dialog: title, message, focus-ordered form, buttons.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dialog {
    pub title: String,
    /// Message lines above the form (`dialog.ts:199`'s `message`).
    pub body: Vec<String>,
    pub fields: Vec<Field>,
    /// Button labels; the last one is always the cancel path.
    pub buttons: Vec<String>,
    /// Focus index into `fields ++ buttons` — the focus trap.
    pub focus: usize,
    /// Validation / action error shown above the buttons.
    pub error: Option<String>,
    /// What submitting runs; `None` for an informational dialog.
    pub pending: Option<PendingPlan>,
}

impl Dialog {
    /// An informational dialog with a single `OK`.
    pub fn message(title: &str, body: Vec<String>) -> Self {
        Dialog {
            title: title.to_string(),
            body,
            fields: Vec::new(),
            buttons: vec!["OK".to_string()],
            focus: 0,
            error: None,
            pending: None,
        }
    }

    /// A `Yes`/`No` confirmation for a gated plan.
    ///
    /// `confirm_label` names the action (`"Delete Branch"`), matching the
    /// reference's `showConfirmation(message, actionName, …)`.
    pub fn confirm(
        title: &str,
        body: Vec<String>,
        confirm_label: &str,
        pending: PendingPlan,
    ) -> Self {
        Dialog {
            title: title.to_string(),
            body,
            fields: Vec::new(),
            buttons: vec![confirm_label.to_string(), "Cancel".to_string()],
            focus: 0,
            error: None,
            pending: Some(pending),
        }
    }

    /// A form whose submit button is the action itself (`showForm`).
    pub fn form(
        title: &str,
        body: Vec<String>,
        fields: Vec<Field>,
        action_label: &str,
        pending: PendingPlan,
    ) -> Self {
        Dialog {
            title: title.to_string(),
            body,
            fields,
            buttons: vec![action_label.to_string(), "Cancel".to_string()],
            focus: 0,
            error: None,
            pending: Some(pending),
        }
    }

    /// Total focusables: the form rows followed by the buttons.
    pub fn focusable_count(&self) -> usize {
        self.fields.len() + self.buttons.len()
    }

    /// The focused field, if the focus is inside the form.
    pub fn focused_field(&self) -> Option<&Field> {
        self.fields.get(self.focus)
    }

    /// The focused button index, if the focus is on a button.
    pub fn focused_button(&self) -> Option<usize> {
        self.focus
            .checked_sub(self.fields.len())
            .filter(|index| *index < self.buttons.len())
    }

    /// Validate the form; `Err` carries the message the dialog shows.
    pub fn validate(&self) -> Result<(), String> {
        for field in &self.fields {
            field.validate()?;
        }
        Ok(())
    }

    /// Route one keypress through the focus trap.
    ///
    /// `Enter` submits from anywhere (C12), `Escape` always closes (C02), and
    /// `Tab`/`Shift+Tab` walk the generated `tabindex` order and wrap.
    pub fn handle(&mut self, key: DialogKey) -> Option<DialogResult> {
        match key {
            DialogKey::Escape => return Some(DialogResult::Cancel),
            DialogKey::Next => {
                self.focus = (self.focus + 1) % self.focusable_count().max(1);
                return None;
            }
            DialogKey::Prev => {
                let count = self.focusable_count().max(1);
                self.focus = (self.focus + count - 1) % count;
                return None;
            }
            DialogKey::Submit => {
                if let Err(message) = self.validate() {
                    self.error = Some(message);
                    return None;
                }
                let button = self.focused_button().unwrap_or(0);
                let values = self.values();
                return Some(DialogResult::Submit { values, button });
            }
            _ => {}
        }

        let index = self.focus;
        let Some(field) = self.fields.get_mut(index) else {
            return None;
        };
        match (key, &mut field.kind) {
            (DialogKey::Char(c), FieldKind::Text { .. }) => field.insert(c),
            (DialogKey::Backspace, FieldKind::Text { .. }) => field.backspace(),
            (DialogKey::Delete, FieldKind::Text { .. }) => field.delete(),
            (DialogKey::Left, FieldKind::Text { .. }) => {
                field.cursor = field.cursor.saturating_sub(1)
            }
            (DialogKey::Right, FieldKind::Text { .. }) => {
                let end = field.value.chars().count();
                field.cursor = (field.cursor + 1).min(end);
            }
            (DialogKey::Home, FieldKind::Text { .. }) => field.cursor = 0,
            (DialogKey::End, FieldKind::Text { .. }) => field.cursor = field.value.chars().count(),
            // A dropdown has no popup: Left/Right cycle the option in place.
            (DialogKey::Left, FieldKind::Select { selected, .. }) => {
                *selected = selected.saturating_sub(1)
            }
            (DialogKey::Right, FieldKind::Select { options, selected }) => {
                *selected = (*selected + 1).min(options.len().saturating_sub(1))
            }
            (DialogKey::Space, FieldKind::Checkbox { checked }) => *checked = !*checked,
            _ => {}
        }
        None
    }

    /// The form values in field order.
    pub fn values(&self) -> Vec<DialogValue> {
        self.fields
            .iter()
            .map(|field| match &field.kind {
                FieldKind::Text { .. } => DialogValue::Text(field.value.clone()),
                FieldKind::Select { options, selected } => DialogValue::Select(
                    options
                        .get(*selected)
                        .cloned()
                        .unwrap_or_else(|| "(none)".to_string()),
                ),
                FieldKind::Checkbox { checked } => DialogValue::Bool(*checked),
            })
            .collect()
    }

    /// Layer the form values onto a copy of the pending target.
    pub fn apply(&self, target: &mut ActionTarget) {
        for field in &self.fields {
            match &field.kind {
                FieldKind::Text { .. } => set_text(target, &field.name, &field.value),
                FieldKind::Select { options, selected } => {
                    if let Some(value) = options.get(*selected) {
                        set_select(target, &field.name, value);
                    }
                }
                FieldKind::Checkbox { checked } => set_flag(target, &field.name, *checked),
            }
        }
    }

    /// The plan to run if the user submits button `button`.
    ///
    /// `None` for an informational dialog, for the cancel button (the last one)
    /// and for a dialog with nothing pending. The plan is **rebuilt** from the
    /// form values, so what runs is exactly what the user filled in.
    pub fn plan_for_submit(&self, button: usize) -> Option<ActionPlan> {
        let pending = self.pending.as_ref()?;
        if self.buttons.len() > 1 && button + 1 == self.buttons.len() {
            return None; // the cancel button
        }
        let mut target = pending.target.clone();
        self.apply(&mut target);
        Some(actions::plan(pending.action, &target))
    }
}

/// Write a text form value back into the matching [`ActionTarget`] field.
///
/// The names match [`crate::menu::MenuContext::action_target`]'s vocabulary, so
/// a form dialog and a menu row speak the same language.
pub fn set_text(target: &mut ActionTarget, name: &str, value: &str) {
    let slot = match name {
        "name" => &mut target.name,
        "url" => &mut target.url,
        "ref_name" => &mut target.ref_name,
        "message" => &mut target.message,
        "new_name" => &mut target.new_name,
        "key" => &mut target.key,
        "value" => &mut target.value,
        "branch" => &mut target.branch,
        "local_branch" => &mut target.local_branch,
        "remote" => &mut target.remote,
        "path" => &mut target.path,
        "commit" => &mut target.commit,
        "from" => &mut target.from,
        "to" => &mut target.to,
        "location" => &mut target.location,
        "selector" => &mut target.selector,
        "subject" => &mut target.subject,
        "hash" => &mut target.hash,
        _ => return,
    };
    *slot = value.to_string();
}

/// Write a dropdown value back into the matching enum field.
pub fn set_select(target: &mut ActionTarget, name: &str, value: &str) {
    match name {
        "reset_mode" => {
            target.reset_mode = match value.to_ascii_lowercase().as_str() {
                "soft" => ResetMode::Soft,
                "hard" => ResetMode::Hard,
                _ => ResetMode::Mixed,
            }
        }
        "push_mode" => {
            target.push_mode = match value.to_ascii_lowercase().as_str() {
                "force" => PushMode::Force,
                "force with lease" | "force-with-lease" => PushMode::ForceWithLease,
                _ => PushMode::Normal,
            }
        }
        _ => set_text(target, name, value),
    }
}

/// Write a checkbox value back into the matching boolean field.
pub fn set_flag(target: &mut ActionTarget, name: &str, value: bool) {
    let slot = match name {
        "force" => &mut target.force,
        "directories" => &mut target.directories,
        "include_untracked" => &mut target.include_untracked,
        "reinstate_index" => &mut target.reinstate_index,
        "record_origin" => &mut target.record_origin,
        "no_commit" => &mut target.no_commit,
        "set_upstream" => &mut target.set_upstream,
        "prune" => &mut target.prune,
        "prune_tags" => &mut target.prune_tags,
        "checkout" => &mut target.checkout,
        "no_ff" => &mut target.no_ff,
        "squash" => &mut target.squash,
        "ignore_date" => &mut target.ignore_date,
        "annotated" => &mut target.annotated,
        _ => return,
    };
    *slot = value;
}

/// The form a gated action needs, or `None` for a plain Yes/No confirmation.
///
/// This is the W6 completion of the `ActionTarget` parameter bag: W5 planned
/// each gated action with whatever the menu already knew, and the rows below
/// are the ones whose `git` argv still needs a value from the user.
pub fn fields_for(action: RefAction, target: &ActionTarget) -> Vec<Field> {
    match action {
        RefAction::AddRemote | RefAction::EditRemote => vec![
            Field::ref_text("name", "Remote Name", &target.name),
            Field::text("url", "Remote URL", &target.url),
        ],
        RefAction::AddTag => vec![
            Field::ref_text("name", "Tag Name", &target.name),
            Field::optional_text("message", "Message", &target.message),
        ],
        RefAction::CreateBranch | RefAction::RenameBranch => {
            vec![Field::ref_text("new_name", "Branch Name", &target.new_name)]
        }
        RefAction::BranchFromStash => {
            vec![Field::ref_text("new_name", "Branch Name", &target.new_name)]
        }
        RefAction::ResetToCommit => vec![Field::select(
            "reset_mode",
            "Reset Mode",
            &["Soft", "Mixed", "Hard"],
            reset_index(target.reset_mode),
        )],
        RefAction::CleanUntrackedFiles => vec![
            Field::checkbox(
                "directories",
                "Also remove directories (-d)",
                target.directories,
            ),
            Field::checkbox(
                "include_untracked",
                "Also remove ignored files (-x)",
                target.include_untracked,
            ),
        ],
        RefAction::PushBranch => vec![Field::checkbox(
            "set_upstream",
            "Set upstream (-u)",
            target.set_upstream,
        )],
        RefAction::Merge => vec![
            Field::checkbox("no_ff", "No fast-forward (--no-ff)", target.no_ff),
            Field::checkbox("squash", "Squash (--squash)", target.squash),
        ],
        RefAction::CherrypickCommit => vec![Field::checkbox(
            "record_origin",
            "Record origin (-x)",
            target.record_origin,
        )],
        _ => Vec::new(),
    }
}

/// The dropdown index for a [`ResetMode`].
fn reset_index(mode: ResetMode) -> usize {
    match mode {
        ResetMode::Soft => 0,
        ResetMode::Mixed => 1,
        ResetMode::Hard => 2,
    }
}

/// Build the dialog a gated plan should show.
///
/// `reason` is the [`crate::actions::PlanKind::Confirm`] payload — the *why*
/// from the W5 plan — and it is shown verbatim so the confirmation text and the
/// audit trail cannot drift apart.
pub fn dialog_for_plan(
    action: RefAction,
    target: &ActionTarget,
    plan: &ActionPlan,
    reason: &str,
) -> Dialog {
    let pending = PendingPlan {
        action,
        target: target.clone(),
        plan: plan.clone(),
    };
    let fields = fields_for(action, target);

    let mut body = vec![reason.to_string()];
    if !plan.argv.is_empty() {
        body.push(format!("git {}", plan.argv.join(" ")));
    }
    body.push(format!("[{} · {}]", plan.dsp, plan.command));

    if fields.is_empty() {
        Dialog::confirm(plan.label, body, plan.label, pending)
    } else {
        Dialog::form(plan.label, body, fields, plan.label, pending)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pending(action: RefAction, target: ActionTarget) -> PendingPlan {
        let plan = actions::plan(action, &target);
        PendingPlan {
            action,
            target,
            plan,
        }
    }

    fn branch_target() -> ActionTarget {
        ActionTarget {
            ref_name: "feature".to_string(),
            branch: "feature".to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn a_confirmation_starts_on_the_confirm_button() {
        let dialog = Dialog::confirm(
            "Delete Branch",
            vec!["deletes a ref".to_string()],
            "Delete Branch",
            pending(RefAction::DeleteBranch, branch_target()),
        );
        assert_eq!(dialog.buttons, vec!["Delete Branch", "Cancel"]);
        assert_eq!(dialog.focus, 0);
        assert_eq!(dialog.focused_button(), Some(0));
        assert!(dialog.focused_field().is_none());
    }

    #[test]
    fn enter_submits_and_escape_cancels() {
        let mut dialog = Dialog::confirm(
            "Delete Branch",
            vec![],
            "Delete Branch",
            pending(RefAction::DeleteBranch, branch_target()),
        );
        assert_eq!(dialog.handle(DialogKey::Escape), Some(DialogResult::Cancel));

        let result = dialog.handle(DialogKey::Submit).expect("submits");
        assert_eq!(
            result,
            DialogResult::Submit {
                values: Vec::new(),
                button: 0
            }
        );
    }

    #[test]
    fn focus_walks_fields_then_buttons_and_wraps() {
        let mut dialog = Dialog::form(
            "Add Remote",
            vec![],
            vec![
                Field::ref_text("name", "Remote Name", "origin"),
                Field::text("url", "Remote URL", "https://example.invalid/r.git"),
            ],
            "Add Remote",
            pending(RefAction::AddRemote, ActionTarget::default()),
        );
        assert_eq!(dialog.focusable_count(), 4); // 2 fields + 2 buttons
        assert_eq!(
            dialog.focused_field().map(|f| f.name.as_str()),
            Some("name")
        );
        dialog.handle(DialogKey::Next);
        assert_eq!(dialog.focused_field().map(|f| f.name.as_str()), Some("url"));
        dialog.handle(DialogKey::Next);
        assert_eq!(dialog.focused_button(), Some(0));
        dialog.handle(DialogKey::Next);
        assert_eq!(dialog.focused_button(), Some(1), "the Cancel button");
        dialog.handle(DialogKey::Next);
        assert_eq!(dialog.focus, 0, "focus wraps back to the first field");
        dialog.handle(DialogKey::Prev);
        assert_eq!(dialog.focused_button(), Some(1), "Prev wraps backwards");
    }

    #[test]
    fn enter_on_a_field_submits_with_the_first_button() {
        let mut dialog = Dialog::form(
            "Add Remote",
            vec![],
            vec![Field::ref_text("name", "Remote Name", "origin")],
            "Add Remote",
            pending(RefAction::AddRemote, ActionTarget::default()),
        );
        let result = dialog.handle(DialogKey::Submit).expect("submits");
        match result {
            DialogResult::Submit { button, .. } => {
                assert_eq!(button, 0, "C12: Enter submits from an input")
            }
            DialogResult::Cancel => panic!("Enter must not cancel"),
        }
    }

    #[test]
    fn enter_on_the_cancel_button_is_the_cancel_path() {
        let mut dialog = Dialog::confirm(
            "Delete Tag",
            vec![],
            "Delete Tag",
            pending(RefAction::DeleteTag, branch_target()),
        );
        dialog.handle(DialogKey::Next); // -> Cancel
        assert_eq!(dialog.focused_button(), Some(1));
        let result = dialog.handle(DialogKey::Submit).expect("submits");
        match result {
            DialogResult::Submit { button, .. } => assert_eq!(button, 1),
            DialogResult::Cancel => panic!("the second button is still a submit"),
        }
        assert!(
            dialog.plan_for_submit(1).is_none(),
            "the cancel button never runs a plan"
        );
    }

    #[test]
    fn a_required_field_blocks_submission_with_a_message() {
        let mut dialog = Dialog::form(
            "Create Branch",
            vec![],
            vec![Field::ref_text("new_name", "Branch Name", "")],
            "Create Branch",
            pending(RefAction::CreateBranch, branch_target()),
        );
        assert_eq!(dialog.handle(DialogKey::Submit), None);
        assert!(
            dialog
                .error
                .as_deref()
                .unwrap_or("")
                .contains("cannot be empty"),
            "{:?}",
            dialog.error
        );
        dialog.fields[0].value = "new-branch".to_string();
        dialog.error = None;
        assert!(dialog.handle(DialogKey::Submit).is_some());
    }

    #[test]
    fn a_ref_field_refuses_whitespace() {
        let mut dialog = Dialog::form(
            "Create Branch",
            vec![],
            vec![Field::ref_text("new_name", "Branch Name", "bad name")],
            "Create Branch",
            pending(RefAction::CreateBranch, branch_target()),
        );
        assert_eq!(dialog.handle(DialogKey::Submit), None);
        assert!(
            dialog.error.as_deref().unwrap_or("").contains("whitespace"),
            "{:?}",
            dialog.error
        );
    }

    #[test]
    fn text_editing_tracks_the_cursor() {
        let mut dialog = Dialog::form(
            "Create Branch",
            vec![],
            vec![Field::ref_text("new_name", "Branch Name", "ab")],
            "Create Branch",
            pending(RefAction::CreateBranch, branch_target()),
        );
        dialog.handle(DialogKey::Left);
        dialog.handle(DialogKey::Char('X'));
        assert_eq!(dialog.fields[0].value, "aXb");
        assert_eq!(dialog.fields[0].cursor, 2);
        dialog.handle(DialogKey::Backspace);
        assert_eq!(dialog.fields[0].value, "ab");
        dialog.handle(DialogKey::Home);
        dialog.handle(DialogKey::Delete);
        assert_eq!(dialog.fields[0].value, "b");
        dialog.handle(DialogKey::End);
        assert_eq!(dialog.fields[0].cursor, 1);
    }

    #[test]
    fn text_editing_is_utf8_safe() {
        let mut dialog = Dialog::form(
            "Create Branch",
            vec![],
            vec![Field::ref_text("new_name", "Branch Name", "café")],
            "Create Branch",
            pending(RefAction::CreateBranch, branch_target()),
        );
        dialog.handle(DialogKey::Backspace);
        assert_eq!(dialog.fields[0].value, "caf");
        assert_eq!(dialog.fields[0].cursor, 3);
    }

    #[test]
    fn a_dropdown_cycles_with_left_and_right() {
        let mut dialog = Dialog::form(
            "Reset",
            vec![],
            vec![Field::select(
                "reset_mode",
                "Reset Mode",
                &["Soft", "Mixed", "Hard"],
                1,
            )],
            "Reset",
            pending(RefAction::ResetToCommit, branch_target()),
        );
        assert_eq!(dialog.fields[0].display_value(), "Mixed");
        dialog.handle(DialogKey::Right);
        assert_eq!(dialog.fields[0].display_value(), "Hard");
        dialog.handle(DialogKey::Right);
        assert_eq!(
            dialog.fields[0].display_value(),
            "Hard",
            "clamps at the end"
        );
        dialog.handle(DialogKey::Left);
        dialog.handle(DialogKey::Left);
        dialog.handle(DialogKey::Left);
        assert_eq!(
            dialog.fields[0].display_value(),
            "Soft",
            "clamps at the start"
        );
    }

    #[test]
    fn a_checkbox_toggles_with_space() {
        let mut dialog = Dialog::form(
            "Clean",
            vec![],
            vec![Field::checkbox("directories", "Also directories", false)],
            "Clean",
            pending(RefAction::CleanUntrackedFiles, branch_target()),
        );
        assert_eq!(dialog.fields[0].display_value(), "[ ]");
        dialog.handle(DialogKey::Space);
        assert_eq!(dialog.fields[0].display_value(), "[x]");
        assert_eq!(dialog.values(), vec![DialogValue::Bool(true)]);
    }

    #[test]
    fn applying_a_form_rewrites_the_plan_argv() {
        let target = branch_target();
        let mut dialog = Dialog::form(
            "Create Branch",
            vec![],
            vec![Field::ref_text("new_name", "Branch Name", "feature")],
            "Create Branch",
            pending(RefAction::CreateBranch, target),
        );
        let base = dialog.pending.as_ref().unwrap().plan.argv.clone();
        assert!(
            base.iter().any(|part| part == "feature"),
            "the W5 plan already carries the menu's value: {base:?}"
        );

        dialog.fields[0].value = "renamed-branch".to_string();
        let plan = dialog.plan_for_submit(0).expect("a plan");
        assert!(
            plan.argv.iter().any(|part| part == "renamed-branch"),
            "the form value reached the argv: {:?}",
            plan.argv
        );
        assert!(
            !plan.argv.iter().any(|part| part == "feature"),
            "the old value is gone: {:?}",
            plan.argv
        );
    }

    #[test]
    fn reset_mode_and_flags_round_trip_into_the_target() {
        let mut dialog = Dialog::form(
            "Reset",
            vec![],
            vec![
                Field::select("reset_mode", "Reset Mode", &["Soft", "Mixed", "Hard"], 2),
                Field::checkbox("force", "Force", true),
            ],
            "Reset",
            pending(RefAction::ResetToCommit, branch_target()),
        );
        let mut target = ActionTarget::default();
        dialog.apply(&mut target);
        assert_eq!(target.reset_mode, ResetMode::Hard);
        assert!(target.force);
        let plan = dialog.plan_for_submit(0).expect("a plan");
        assert!(
            plan.argv.iter().any(|part| part == "--hard"),
            "{:?}",
            plan.argv
        );
    }

    #[test]
    fn push_mode_select_maps_to_the_enum() {
        let mut target = ActionTarget::default();
        set_select(&mut target, "push_mode", "Force");
        assert_eq!(target.push_mode, PushMode::Force);
        set_select(&mut target, "push_mode", "Force With Lease");
        assert_eq!(target.push_mode, PushMode::ForceWithLease);
        set_select(&mut target, "push_mode", "Normal");
        assert_eq!(target.push_mode, PushMode::Normal);
    }

    #[test]
    fn every_gated_action_gets_a_dialog() {
        let target = branch_target();
        let mut confirmed = 0;
        for action in RefAction::ALL {
            let plan = actions::plan(action, &target);
            if !plan.kind.is_gated() {
                continue;
            }
            confirmed += 1;
            let reason = match plan.kind {
                crate::actions::PlanKind::Confirm(reason) => reason,
                _ => unreachable!(),
            };
            let dialog = dialog_for_plan(action, &target, &plan, reason);
            assert_eq!(dialog.title, plan.label);
            assert!(
                dialog.body.iter().any(|line| line == reason),
                "{action:?} must show the plan's reason"
            );
            assert!(dialog.pending.is_some(), "{action:?} must be runnable");
            assert!(dialog.plan_for_submit(0).is_some(), "{action:?}");
            assert!(dialog.plan_for_submit(1).is_none(), "{action:?} cancel");
        }
        assert_eq!(confirmed, 25, "the W5 gate count must not drift");
    }

    #[test]
    fn a_dialog_without_pending_never_produces_a_plan() {
        let dialog = Dialog::message("About", vec!["nothing to do".to_string()]);
        assert_eq!(dialog.buttons, vec!["OK"]);
        assert!(dialog.plan_for_submit(0).is_none());
        assert!(dialog.validate().is_ok());
    }

    #[test]
    fn add_remote_asks_for_a_name_and_a_url() {
        let target = ActionTarget::default();
        let fields = fields_for(RefAction::AddRemote, &target);
        assert_eq!(fields.len(), 2);
        assert_eq!(fields[0].name, "name");
        assert_eq!(fields[1].name, "url");
        assert!(fields[0].required && fields[1].required);
    }
}
