/**
 * Scope head blocks plug into `policy_hat`'s PRINCIPAL/RESOURCE/ACTION slots.
 *
 * Coverage:
 *   PRINCIPAL/RESOURCE (output: "Scope")
 *     scope_all   — bare `principal` / `resource`
 *     scope_eq    — `== Type::"id"`
 *     scope_in    — `in  Type::"id"`
 *     scope_is    — `is Type` (+ optional `in Type::"id"` qualifier; both fields
 *                              must be filled to activate, else treated as no-qualifier)
 *     scope_slot  — `?principal` / `?resource` template slot
 *
 *   ACTION (output: "ActionScope")
 *     action_scope_all     — bare `action`
 *     action_scope_eq      — `== Action::"id"`
 *     action_scope_in      — `in [Action::"a", Action::"b", ...]`; children are
 *                            `action_scope_in_item` blocks chained in the ITEMS
 *                            statement list. Empty list serialises as `in []`.
 *     action_scope_in_item — child wrapper (previous/next "ActionScopeInItem"),
 *                            carries one EntityRef as TYPE/ID text fields.
 *
 * Exported as factories so tooltips resolve through i18n at registration
 * time, not at module import.
 */

import { i18n } from "../../i18n";

// ── PRINCIPAL / RESOURCE scope ──────────────────────────────────────────

export const SCOPE_BLOCK_JSON = () =>
  ({
    type: "scope_all",
    message0: "any",
    output: "Scope",
    colour: 200,
    tooltip: i18n.t("blocks:block.scope_all.tooltip"),
  }) as const;

export const SCOPE_EQ_BLOCK_JSON = () =>
  ({
    type: "scope_eq",
    message0: '== %1 :: "%2"',
    args0: [
      { type: "field_input", name: "TYPE", text: "User" },
      { type: "field_input", name: "ID", text: "alice" },
    ],
    output: "Scope",
    colour: 200,
    tooltip: i18n.t("blocks:block.scope_eq.tooltip"),
  }) as const;

export const SCOPE_IN_BLOCK_JSON = () =>
  ({
    type: "scope_in",
    message0: 'in %1 :: "%2"',
    args0: [
      { type: "field_input", name: "TYPE", text: "Group" },
      { type: "field_input", name: "ID", text: "admins" },
    ],
    output: "Scope",
    colour: 200,
    tooltip: i18n.t("blocks:block.scope_in.tooltip"),
  }) as const;

export const SCOPE_IS_BLOCK_JSON = () =>
  ({
    type: "scope_is",
    message0: 'is %1   in %2 :: "%3"',
    args0: [
      { type: "field_input", name: "TYPE", text: "User" },
      // Optional qualifier — leave both blank to skip the `in` clause.
      { type: "field_input", name: "IN_TYPE", text: "" },
      { type: "field_input", name: "IN_ID", text: "" },
    ],
    output: "Scope",
    colour: 200,
    tooltip: i18n.t("blocks:block.scope_is.tooltip"),
  }) as const;

export const SCOPE_SLOT_BLOCK_JSON = () =>
  ({
    type: "scope_slot",
    message0: "%1",
    args0: [
      {
        type: "field_dropdown",
        name: "SLOT",
        options: [
          ["?principal", "?principal"],
          ["?resource", "?resource"],
        ],
      },
    ],
    output: "Scope",
    colour: 200,
    tooltip: i18n.t("blocks:block.scope_slot.tooltip"),
  }) as const;

// ── ACTION scope ────────────────────────────────────────────────────────

export const ACTION_SCOPE_BLOCK_JSON = () =>
  ({
    type: "action_scope_all",
    message0: "any action",
    output: "ActionScope",
    colour: 200,
    tooltip: i18n.t("blocks:block.action_scope_all.tooltip"),
  }) as const;

export const ACTION_SCOPE_EQ_BLOCK_JSON = () =>
  ({
    type: "action_scope_eq",
    message0: '== %1 :: "%2"',
    args0: [
      // Namespace + entity type. Default "Action"; many shipped schemas use
      // namespaced forms like "Token::Action" / "Amm::Action" — the user
      // must type the qualifier matching the schema, or schema validation
      // rejects the policy.
      { type: "field_input", name: "TYPE", text: "Action" },
      { type: "field_input", name: "ID", text: "Swap" },
    ],
    output: "ActionScope",
    colour: 200,
    tooltip: i18n.t("blocks:block.action_scope_eq.tooltip"),
  }) as const;

export const ACTION_SCOPE_IN_BLOCK_JSON = () =>
  ({
    type: "action_scope_in",
    message0: "in [ %1 ]",
    args0: [{ type: "input_statement", name: "ITEMS", check: "ActionScopeInItem" }],
    output: "ActionScope",
    colour: 200,
    tooltip: i18n.t("blocks:block.action_scope_in.tooltip"),
  }) as const;

export const ACTION_SCOPE_IN_ITEM_BLOCK_JSON = () =>
  ({
    type: "action_scope_in_item",
    message0: '%1 :: "%2"',
    args0: [
      { type: "field_input", name: "TYPE", text: "Action" },
      { type: "field_input", name: "ID", text: "" },
    ],
    previousStatement: "ActionScopeInItem",
    nextStatement: "ActionScopeInItem",
    colour: 200,
    tooltip: i18n.t("blocks:block.action_scope_in_item.tooltip"),
  }) as const;
