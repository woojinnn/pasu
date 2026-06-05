// blocks: registering as an EigenLayer operator with a zero-address delegationApprover (anyone may delegate to you without your approval).
// why:    a zero `delegationApprover` disables the per-delegation approval gate, so any staker can delegate into your operator unconditionally; that opens you to unwanted stake (and its slashing exposure / griefing) being attached without consent (open-approver / unconstrained-delegation threat class). Source: unit410 EigenLayer operator-config guidance.
// bucket: protocol/eigenlayer
// tags: bucket:protocol cat:eigenlayer static venue:eigenlayer action-family:restaking
//
// Pure-static. `delegationApprover` is a required field (VOCAB §4 restaking
// RegisterOperator); the zero address means "no approval required to delegate".
@id("eigenlayer-register-operator-open-approver")
@severity("warn")
@reason("Registering as an operator with an open (zero-address) delegationApprover — anyone could delegate stake to you without approval")
forbid(principal, action == Restaking::Action::"RegisterOperator", resource)
when {
  context.delegationApprover == "0x0000000000000000000000000000000000000000"
};
