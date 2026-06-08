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
export { formToIr, irToForm, leafToExpr } from "./convert";
export {
  fieldsForTrigger,
  operatorsFor,
  valueKindForField,
  KNOWN_ACTIONS,
  type FieldOption,
  type KnownAction,
} from "./field-catalog";
