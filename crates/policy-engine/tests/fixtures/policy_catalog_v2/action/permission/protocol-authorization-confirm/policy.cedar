// blocks: silently granting a protocol-level operator/relayer/manager role that can move funds on your behalf
// why:    over-broad delegated authorization (e.g. Balancer relayer / operator role) — a granted operator acts for you (access-control / permission-delegation threat class)
// bucket: action/permission  (precedence-won)
// tags: bucket:action cat:permission static
// Catalog G1 — confirm a protocol-level authorization grant (e.g. Balancer relayer,
// operator/manager role). Warn only when GRANTING (`isAuthorized == true`); revokes pass.
// Pure-static.
@id("protocol-authorization-confirm")
@severity("warn")
@reason("Granting a protocol-level authorization (relayer/operator/manager) — confirm the grantee")
forbid(principal, action == Permission::Action::"ProtocolAuthorization", resource)
when { context.isAuthorized == true };
