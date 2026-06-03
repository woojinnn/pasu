// blocks: registering your wallet as an EigenLayer operator without an explicit confirm.
// why:    `registerAsOperator` permanently turns your address into a delegation target that accepts other stakers' shares and takes on slashing accountability + an on-chain allocation-delay commitment; this is a role-assumption decision a retail signer rarely means to make blindly (privileged-role assumption threat class). Source: EigenLayer DelegationManager docs.
// bucket: protocol/eigenlayer
// tags: bucket:protocol cat:eigenlayer static venue:eigenlayer action-family:restaking
//
// Pure-static — anchor an explicit confirm on operator registration.
@id("eigenlayer-register-as-operator-confirm")
@severity("warn")
@reason("Registering your wallet as an EigenLayer operator — confirm you intend to take on operator (slashing-accountable) status")
forbid(principal, action == Restaking::Action::"RegisterOperator", resource);
