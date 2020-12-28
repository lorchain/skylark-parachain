#![cfg_attr(not(feature = "std"), no_std)]

use codec::{Encode, Decode};
use sp_std::{
	convert::{TryInto}
};
use frame_support::{
	decl_module, decl_storage, decl_event, ensure, dispatch::DispatchResult,
	traits::{
		Currency, ReservableCurrency,
		OnUnbalanced, Get,
	},
	StorageValue, StorageMap, Parameter,
	weights::{Weight, DispatchClass},
};
use sp_runtime::traits::{
	Member, AtLeast32Bit, Bounded, CheckedAdd, CheckedSub, One,
};
use frame_system::{self as system, ensure_signed};
use pallet_timestamp as timestamp;

/// The module's configuration trait.
pub trait Trait: system::Trait + timestamp::Trait{
	/// Curated group ID
	type CuratedGroupId: Parameter + Member + Default + Bounded + AtLeast32Bit + Copy;

	/// Content hash
	type ContentHash: Parameter + Member + Default + Copy;

	/// Currency type for this module.
	type Currency: ReservableCurrency<Self::AccountId>;

	/// The overarching event type.
	type Event: From<Event<Self>> + Into<<Self as system::Trait>::Event>;

	/// Handler for the unbalanced reduction when slashing a staker.
	type Slash: OnUnbalanced<NegativeImbalanceOf<Self>>;

	/// Handler for the unbalanced increment when rewarding a staker.
	type Reward: OnUnbalanced<PositiveImbalanceOf<Self>>;

	/// Cost of new curated group creation.
	type CuratedGroupCreationFee: Get<BalanceOf<Self>>;
}

// Balance zone
pub type BalanceOf<T> = <<T as Trait>::Currency as Currency<<T as system::Trait>::AccountId>>::Balance;
type PositiveImbalanceOf<T> =
<<T as Trait>::Currency as Currency<<T as system::Trait>::AccountId>>::PositiveImbalance;
type NegativeImbalanceOf<T> =
<<T as Trait>::Currency as Currency<<T as system::Trait>::AccountId>>::NegativeImbalance;

#[cfg_attr(feature ="std", derive(Debug, PartialEq, Eq))]
#[derive(Encode, Decode)]
pub struct CuratedGroup<Balance, Moment, ContentHash> {
	threshold: u64,
	pub min_deposit: Balance,
	pub apply_stage_len: Moment,
	pub commit_stage_len: Moment,
	pub content_hash: ContentHash,
}

// This module's storage items.
decl_storage! {
  trait Store for Module<T: Trait> as CuratedGroup {
	// Create
    CuratedGroups get(fn curated_groups): map hasher(blake2_128_concat) T::CuratedGroupId => Option<CuratedGroup<BalanceOf<T>, T::Moment, T::ContentHash>>;
	NextCuratedGroupId get(fn next_curated_group_id): T::CuratedGroupId;

    // Stake
    StakedAmount get(fn staked_amount): map hasher(blake2_128_concat) (T::CuratedGroupId, T::AccountId) => BalanceOf<T>;
    TotalStakedAmount get(fn total_staked_amount): map hasher(blake2_128_concat) T::CuratedGroupId => BalanceOf<T>;

    // Invest
    InvestedAmount get(fn invested_amount): map hasher(blake2_128_concat) (T::CuratedGroupId, T::AccountId) => BalanceOf<T>;
    TotalInvestedAmount get(fn total_invested_amount): map hasher(blake2_128_concat) T::CuratedGroupId => BalanceOf<T>;
  }
}

// The module's dispatchable functions.
decl_module! {
  /// The module declaration.
  pub struct Module<T: Trait> for enum Call where origin: T::Origin {

    fn deposit_event() = default;

    /// create curation with rules
    #[weight = 100_000]
    pub fn create(origin, content_hash: T::ContentHash, amount: BalanceOf<T>) -> DispatchResult {
      let who = ensure_signed(origin)?;
      Self::do_create(who.clone(), content_hash, amount)
    }

    /// stake
    #[weight = 100_000]
    pub fn stake(origin, id: T::CuratedGroupId, amount: BalanceOf<T>) -> DispatchResult {
      let who = ensure_signed(origin)?;
      ensure!(<CuratedGroups<T>>::contains_key(id), "Curated group does not exist");

      Self::do_stake(who.clone(), id, amount)
    }

    #[weight = 100_000]
    pub fn withdraw(origin, id: T::CuratedGroupId, amount: BalanceOf<T>) -> DispatchResult {
      let who = ensure_signed(origin)?;
      ensure!(<CuratedGroups<T>>::contains_key(id), "Curated group does not exist");

      Self::do_withdraw(who.clone(), id, amount)
    }

    #[weight = 100_000]
    pub fn invest(origin, id: T::CuratedGroupId, amount: BalanceOf<T>) -> DispatchResult {
      let who = ensure_signed(origin)?;
      ensure!(<CuratedGroups<T>>::contains_key(id), "Curated group does not exist");

      Self::do_invest(who.clone(), id, amount)
    }

    #[weight = 100_000]
    pub fn update_rules(origin) -> DispatchResult {
      Ok(())
    }

  }
}

decl_event!(
  pub enum Event<T>
  where
    AccountId = <T as system::Trait>::AccountId,
    Balance = BalanceOf<T>,
    <T as Trait>::CuratedGroupId,
    ContentHash = <T as Trait>::ContentHash,
  {
    Created(AccountId, CuratedGroupId, Balance, ContentHash),
    Staked(AccountId, CuratedGroupId, Balance),
    Invested(AccountId, CuratedGroupId, Balance),
    Withdrawed(AccountId, CuratedGroupId, Balance, Balance),
  }
);

impl<T: Trait> Module<T> {
	pub fn is_member_of_curated_group(curated_group_id: T::CuratedGroupId, who: T::AccountId) -> bool {
		let invested = Self::invested_amount((curated_group_id, who.clone()));
		let staked = Self::staked_amount((curated_group_id, who.clone()));
		(invested != <BalanceOf<T>>::from(0)) || (staked != <BalanceOf<T>>::from(0))
	}

	pub fn do_create(who: T::AccountId, content_hash: T::ContentHash, balance: BalanceOf<T>) -> DispatchResult {
		let curated_group_id = <NextCuratedGroupId<T>>::get();

		// reserve some balance
		ensure!(balance >= T::CuratedGroupCreationFee::get(), "deposit should more than curated group creation fee.");
		ensure!(T::Currency::can_reserve(&who, balance), "Balance not enough for creating new curated group.");
		T::Currency::reserve(&who, balance)?;

		let curated_group = CuratedGroup::<BalanceOf<T>, T::Moment, T::ContentHash> {
			threshold: 0,
			min_deposit: BalanceOf::<T>::from(3000),
			apply_stage_len: T::Moment::from(60000),
			commit_stage_len: T::Moment::from(60000),
			content_hash,
		};

		<InvestedAmount<T>>::insert((curated_group_id, who.clone()), balance);
		<TotalInvestedAmount<T>>::insert(curated_group_id, balance);
		<CuratedGroups<T>>::insert(curated_group_id, curated_group);
		<NextCuratedGroupId<T>>::mutate(|id| *id += <T::CuratedGroupId as One>::one());

		Self::deposit_event(RawEvent::Created(who, curated_group_id, balance, content_hash));

		Ok(())
	}

	pub fn do_stake(who: T::AccountId, id: T::CuratedGroupId, amount: BalanceOf<T>) -> DispatchResult {
		// check if enough balance
		ensure!(T::Currency::can_reserve(&who, amount), "Balance not enough.");
		// check if overflow
		let staked_amount = Self::staked_amount((id, who.clone()));
		let total_staked_amount = Self::total_staked_amount(id);
		let new_staked_amount = staked_amount.checked_add(&amount).ok_or("Overflow stake amount")?;
		let new_total_staked_amount = total_staked_amount.checked_add(&amount).ok_or("Overflow total stake amount")?;
		// actual stake
		T::Currency::reserve(&who, amount)?;

		<StakedAmount<T>>::insert((id, who.clone()), new_staked_amount);
		<TotalStakedAmount<T>>::insert(id, new_total_staked_amount);

		Self::deposit_event(RawEvent::Staked(who, id, amount));

		Ok(())
	}

	pub fn do_withdraw(who: T::AccountId, id: T::CuratedGroupId, amount: BalanceOf<T>) -> DispatchResult {
		// check if reserved balance is enough
		// staked_amount is the balance staked(reserved) to this CuratedGroupId
		let staked_amount = Self::staked_amount((id, who.clone()));
		ensure!(staked_amount >= amount, "Can not withdraw more than you have staked.");
		// balance_reserved is the total reserved balance of this account,
		// it consists of staked and invested balance not only to this CuratedGroupId,
		// but to other CuratedGroupIds as well
		let balance_reserved = T::Currency::reserved_balance(&who);
		ensure!(balance_reserved >= staked_amount, "Total reserved balance should more than staked balance.");
		// deduct staked amount
		let new_staked_amount = staked_amount.checked_sub(&amount).ok_or("Underflow staked amount.")?;
		let total_staked_amount = Self::total_staked_amount(id);
		let new_total_staked_amount = total_staked_amount.checked_sub(&amount).ok_or("Underflow total staked amount.")?;
		// actual unreserve balance
		T::Currency::unreserve(&who, amount);

		<StakedAmount<T>>::insert((id, who.clone()), new_staked_amount);
		<TotalStakedAmount<T>>::insert(id, new_total_staked_amount);

		let remained_amount = staked_amount - amount;
		Self::deposit_event(RawEvent::Withdrawed(who, id, amount, remained_amount));

		Ok(())
	}

	pub fn do_invest(who: T::AccountId, id: T::CuratedGroupId, amount: BalanceOf<T>) -> DispatchResult {
		// invest, check if enough balance
		ensure!(T::Currency::can_reserve(&who, amount), "Balance not enough.");
		// check if overflow
		let invested_amount = Self::invested_amount((id, who.clone()));
		let total_invested_amount = Self::total_invested_amount(id);
		let new_invested_amount = invested_amount.checked_add(&amount).ok_or("Overflow stake amount")?;
		let new_total_invested_amount = total_invested_amount.checked_add(&amount).ok_or("Overflow total stake amount")?;

		// actual invest
		T::Currency::reserve(&who, amount)?;

		<InvestedAmount<T>>::insert((id, who.clone()), new_invested_amount);
		<TotalInvestedAmount<T>>::insert(id, new_total_invested_amount);

		Self::deposit_event(RawEvent::Invested(who, id, amount));

		Ok(())
	}
}
