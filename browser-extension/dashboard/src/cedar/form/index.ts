/**
 * Form-editor core: a constrained FormModel and its lossless mapping to/from
 * `PolicyIR`. UI (PolicyFormPane) and the field catalog build on these.
 */
export type {
  FormModel,
  FormGroup,
  FormLeaf,
  FormValue,
  FormTrigger,
  FormSeverity,
  FormOp,
} from "./model";
export { emptyFormModel } from "./model";
export { formToIr, irToForm } from "./convert";
