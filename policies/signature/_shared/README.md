# Shared Signature Policies

Cedar policy text is loaded as complete source strings by `PolicyEngine`; this
workspace does not have a Cedar include/preprocessor layer.

Canonical duplicated signature policies therefore live in this `_shared`
directory. Per-scheme files with the old names are thin comment pointers so
existing paths still explain where the policy moved, while tests and policy
bundles load these canonical `_shared/*.cedar` files for behavior.
