//! Some configurable implementations as associated type for the substrate runtime.

use pallet_primitives::Balance;
use sp_runtime::traits::{Convert, Saturating};
use sp_runtime::{FixedPointNumber, Perquintill};
use frame_support::traits::{OnUnbalanced, Currency, Get};
use pallet_transaction_payment::Multiplier;
// use crate::{Balances, System, Authorship, MaximumBlockWeight, NegativeImbalance};
use crate::{Balances, System, MaximumBlockWeight};

// pub struct Author;
// impl OnUnbalanced<NegativeImbalance> for Author {
// 	fn on_nonzero_unbalanced(amount: NegativeImbalance) {
// 		Balances::resolve_creating(&Authorship::author(), amount);
// 	}
// }

/// Struct that handles the conversion of Balance -> `u64`. This is used for staking's election
/// calculation.
pub struct CurrencyToVoteHandler;

impl CurrencyToVoteHandler {
	fn factor() -> Balance { (Balances::total_issuance() / u64::max_value() as Balance).max(1) }
}

impl Convert<Balance, u64> for CurrencyToVoteHandler {
	fn convert(x: Balance) -> u64 { (x / Self::factor()) as u64 }
}

impl Convert<u128, Balance> for CurrencyToVoteHandler {
	fn convert(x: u128) -> Balance { x * Self::factor() }
}

/// Update the given multiplier based on the following formula
///
///   diff = (previous_block_weight - target_weight)/max_weight
///   v = 0.00004
///   next_weight = weight * (1 + (v * diff) + (v * diff)^2 / 2)
///
/// Where `target_weight` must be given as the `Get` implementation of the `T` generic type.
/// https://research.web3.foundation/en/latest/polkadot/Token%20Economics/#relay-chain-transaction-fees
pub struct TargetedFeeAdjustment<T>(sp_std::marker::PhantomData<T>);

impl<T: Get<Perquintill>> Convert<Multiplier, Multiplier> for TargetedFeeAdjustment<T> {
	fn convert(multiplier: Multiplier) -> Multiplier {
		let max_weight = MaximumBlockWeight::get();
		let block_weight = System::block_weight().total().min(max_weight);
		let target_weight = (T::get() * max_weight) as u128;
		let block_weight = block_weight as u128;

		// determines if the first_term is positive
		let positive = block_weight >= target_weight;
		let diff_abs = block_weight.max(target_weight) - block_weight.min(target_weight);
		// safe, diff_abs cannot exceed u64.
		let diff = Multiplier::saturating_from_rational(diff_abs, max_weight.max(1));
		let diff_squared = diff.saturating_mul(diff);

		// 0.00004 = 4/100_000 = 40_000/10^9
		let v = Multiplier::saturating_from_rational(4, 100_000);
		// 0.00004^2 = 16/10^10 Taking the future /2 into account... 8/10^10
		let v_squared_2 = Multiplier::saturating_from_rational(8, 10_000_000_000u64);

		let first_term = v.saturating_mul(diff);
		let second_term = v_squared_2.saturating_mul(diff_squared);

		if positive {
			// Note: this is merely bounded by how big the multiplier and the inner value can go,
			// not by any economical reasoning.
			let excess = first_term.saturating_add(second_term);
			multiplier.saturating_add(excess)
		} else {
			// Defensive-only: first_term > second_term. Safe subtraction.
			let negative = first_term.saturating_sub(second_term);
			multiplier.saturating_sub(negative)
				// despite the fact that apply_to saturates weight (final fee cannot go below 0)
				// it is crucially important to stop here and don't further reduce the weight fee
				// multiplier. While at -1, it means that the network is so un-congested that all
				// transactions have no weight fee. We stop here and only increase if the network
				// became more busy.
				.max(Multiplier::saturating_from_integer(-1))
		}
	}
}

/// Handles converting a scalar to convert balance
///
pub struct ConvertBalance;
impl Convert<Balance, Balance> for ConvertBalance {
	fn convert(x: Balance) -> Balance {
		x.into()
	}
}
