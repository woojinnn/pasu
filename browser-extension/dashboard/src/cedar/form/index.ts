/**
 * Form-editor core: a constrained FormModel and its lossless mapping to/from
 * `PolicyIR`. UI (PolicyFormPane) and the field catalog build on these.
 */
export type {
  FormModel,
  FormCondition,
  FormGroupNode,
  FormNode,
  FormLeaf,
  FormValue,
  FormTrigger,
  FormSeverity,
  FormOp,
  GroupOp,
} from "./model";
export { emptyFormModel, isGroupNode } from "./model";
export { formToIr, formToIrWithMap, irToForm, leafToExpr, splitRuns, type FormIrMaps } from "./convert";
export { situationsOf, flattenSituations, moveCondTo, type DropTarget } from "./situations";
export {
  fieldsForTrigger,
  operatorsFor,
  valueKindForField,
  KNOWN_ACTIONS,
  ACTION_GROUPS,
  type FieldOption,
  type KnownAction,
} from "./field-catalog";
