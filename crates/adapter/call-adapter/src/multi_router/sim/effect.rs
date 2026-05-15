//! Vocabulary the simulator uses to describe asset movement.
//!
//! Each opcode/envelope is translated into a list of `Effect`s; the
//! [`super::ledger::Ledger`] applies them and aggregates net deltas. Keeping
//! this layer fork- and protocol-agnostic means new envelope types or new
//! routers only need an `effects_of_*` function — the engine and the ledger
//! are reused.

use alloy_primitives::U256;
use policy_engine::action::Address;

/// One movement of an asset in the simulator. The actor refs are abstract
/// (User / Router / External(addr)) so each `effects_of_*` function can
/// emit them without knowing the concrete addresses; resolution to actual
/// `Address` happens inside the ledger using the current `CallContext`.
#[derive(Debug, Clone)]
pub(in crate::multi_router) enum Effect {
    /// Equivalent to `Burn(from, asset, amount)` + `Mint(to, asset, amount)`.
    /// Use when one party hands the asset to another in a single step
    /// (e.g. `transfer`, V4 `TAKE`).
    Move {
        from: ActorRef,
        to: ActorRef,
        asset: Asset,
        amount: AmountSpec,
    },
    /// Asset disappears from `from` (no counterparty in the simulator's
    /// world — typically because the receiving side is outside our model).
    /// Use for "the user spent gas" or "tokens left to a contract we don't
    /// model".
    #[allow(dead_code)]
    Burn {
        from: ActorRef,
        asset: Asset,
        amount: AmountSpec,
    },
    /// Asset appears at `to` from outside the simulator (e.g. minting from a
    /// pool's reserves). Used for the receiving side of a swap when the
    /// router doesn't pre-fund a known account.
    #[allow(dead_code)]
    Mint {
        to: ActorRef,
        asset: Asset,
        amount: AmountSpec,
    },
}

/// Symbolic actor — resolved to a concrete `Address` by
/// [`super::ledger::Ledger::resolve`] using the current `CallContext`.
/// Decoupling lets `effects_of_*` functions stay pure (no ctx capture).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(in crate::multi_router) enum ActorRef {
    /// Resolves to `ctx.from`.
    User,
    /// Resolves to `ctx.to` (the router contract receiving `execute(...)`).
    Router,
    /// Concrete external address — recipient field, fee collector, etc.
    External(Address),
}

/// What asset is moving. Native ETH is `Native`; ERC-20 is keyed only by
/// address (chain id is implicit in the surrounding `CallContext`). Token
/// metadata (symbol/decimals) lives outside the simulator.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(in crate::multi_router) enum Asset {
    Native,
    Erc20(Address),
}

/// How precisely the amount is known. Mirrors `AmountConstraint::kind`
/// but lives in the simulator domain so the ledger arithmetic doesn't
/// depend on the policy schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::multi_router) enum AmountSpec {
    /// Exact value — known from calldata (e.g. `amountIn` of an exact-in swap).
    Exact(U256),
    /// At-least bound — used for slippage-protected outputs (`amountOutMin`).
    AtLeast(U256),
    /// At-most bound — used for slippage-protected inputs (`amountInMax`).
    AtMost(U256),
}

impl AmountSpec {
    pub(in crate::multi_router) fn value(self) -> U256 {
        match self {
            Self::Exact(v) | Self::AtLeast(v) | Self::AtMost(v) => v,
        }
    }
}
