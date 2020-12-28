#![cfg_attr(not(feature = "std"), no_std)]

use sp_runtime::traits::{
	Member, AtLeast32Bit, Bounded, CheckedAdd, CheckedMul, Convert, SaturatedConversion, One,
};
use frame_support::{
	decl_module, decl_storage, decl_event, ensure,
	StorageValue, StorageMap, Parameter,
	traits::{
		Currency, LockableCurrency, WithdrawReasons, LockIdentifier, ReservableCurrency,
	},
	dispatch::{DispatchResult, DispatchError},
	weights::{Weight, DispatchClass},
};
use frame_system::{self as system, ensure_signed};
use pallet_timestamp as timestamp;
use codec::{Encode, Decode};
use sp_std::{cmp, result, convert::{TryInto}};
use curated_group;
use creation;

/// The module's configuration trait.
pub trait Trait: system::Trait + timestamp::Trait + curated_group::Trait + creation::Trait {
	type TcxId:  Parameter + Member + Default + Bounded + AtLeast32Bit + Copy;
	type TcxType: Parameter + Member + Default + Copy;
	type ActionId: Parameter + Member + Default + Copy;
	type ListingId:  Parameter + Member + Default + Bounded + AtLeast32Bit + Copy;
	type ChallengeId: Parameter + Member + Default + Bounded + AtLeast32Bit + Copy;

	/// Currency of this module
	type Currency: LockableCurrency<Self::AccountId, Moment=Self::BlockNumber> + ReservableCurrency<Self::AccountId>;

	/// Convert Balance
	type ConvertBalance: Convert<curated_group::BalanceOf<Self>, BalanceOf<Self>>;

	/// The overarching event type.
	type Event: From<Event<Self>> + Into<<Self as system::Trait>::Event>;
}

// Balance zone
pub type BalanceOf<T> = <<T as Trait>::Currency as Currency<<T as system::Trait>::AccountId>>::Balance;

#[cfg_attr(feature ="std", derive(Debug, PartialEq, Eq))]
#[derive(Encode, Decode)]
pub struct Tcx<CuratedGroupId, TcxType, ContentHash> {
	pub curated_group_id: CuratedGroupId,
	pub tcx_type: TcxType,
	pub content_hash: ContentHash,
}

#[derive(Encode, Decode, Default, Clone, PartialEq)]
#[cfg_attr(feature = "std", derive(Debug))]
pub struct Listing<ListingId, CreationId, Balance, Moment,
	ChallengeId, AccountId> {
	id: ListingId,
	creation_id: CreationId,
	amount: Balance,
	quota: Balance,
	application_expiry: Moment,
	whitelisted: bool,
	challenge_id: ChallengeId,
	owner: AccountId,
}

#[derive(Encode, Decode, Default, Clone, PartialEq)]
#[cfg_attr(feature = "std", derive(Debug))]
pub struct Challenge<Balance, Moment, AccountId, TcxId> {
	amount: Balance,
	quota: Balance,
	voting_ends: Moment,
	resolved: bool,
	reward_pool: Balance,
	total_tokens: Balance,
	owner: AccountId,
	tcx_id: TcxId, // to check if voter is a member of curated_group
}

#[derive(Encode, Decode, Default, Clone, PartialEq)]
#[cfg_attr(feature = "std", derive(Debug))]
pub struct Vote<Balance> {
	value: bool,
	amount: Balance,
	quota: Balance,
	claimed: bool,
}

#[derive(Encode, Decode, Default, Clone, PartialEq)]
#[cfg_attr(feature = "std", derive(Debug))]
pub struct Poll<Balance> {
	votes_for: Balance,
	quota_for: Balance,
	votes_against: Balance,
	quota_against: Balance,
	passed: bool,
}

// This module's storage items.
decl_storage! {
  trait Store for Module<T: Trait> as Tcx {
    Tcxs get(fn tcxs): map hasher(blake2_128_concat) T::TcxId => Option<Tcx<T::CuratedGroupId, T::TcxType, T::ContentHash>>;
    NextTcxId get(fn next_tcx_id): T::TcxId;

    TcxOwner get(fn owner_of): map hasher(blake2_128_concat) T::TcxId => Option<T::CuratedGroupId>;

    OwnedTcxs get(fn owned_tcxs): map hasher(blake2_128_concat) (T::CuratedGroupId, T::TcxId) => T::TcxId;
	NextOwnedTcxId get(fn next_owned_tcx_id): map hasher(blake2_128_concat) T::CuratedGroupId => T::TcxId;

    // actual tcx
    TcxListings get(fn tcx_listings): map hasher(blake2_128_concat) (T::TcxId, T::CreationId) => Listing<T::ListingId, T::CreationId, BalanceOf<T>, T::Moment, T::ChallengeId, T::AccountId>;
	NextTcxListingId get(fn next_tcx_listing_id): map hasher(blake2_128_concat) T::TcxId => T::ListingId;
    TcxListingIndexOpus get(fn tcx_listing_index_creation): map hasher(blake2_128_concat) (T::TcxId, T::ListingId) => T::CreationId;

    Challenges get(fn challenges): map hasher(blake2_128_concat) T::ChallengeId => Challenge<BalanceOf<T>, T::Moment, T::AccountId, T::TcxId>;
    Votes get(fn votes): map hasher(blake2_128_concat) (T::ChallengeId, T::AccountId) => Vote<BalanceOf<T>>;
    Polls get(fn polls): map hasher(blake2_128_concat) T::ChallengeId => Poll<BalanceOf<T>>;

    ChallengeNonce get(fn challenge_nonce): T::ChallengeId;
  }
}

// The module's dispatchable functions.
decl_module! {
  /// The module declaration.
  pub struct Module<T: Trait> for enum Call where origin: T::Origin {
    // Initializing events
    // this is needed only if you are using events in your module
    fn deposit_event() = default;

    #[weight = 100_000]
    pub fn create(origin, curated_group_id: T::CuratedGroupId, tcx_type: T::TcxType, content_hash: T::ContentHash) -> DispatchResult {
		// TODO: check if curated_group agrees
		let curated_group = <curated_group::Module<T>>::curated_groups(curated_group_id).ok_or("Curated group does not exist")?;

		let tcx_id = <NextTcxId<T>>::get();

		let owned_tcx_id = <NextOwnedTcxId<T>>::get(curated_group_id);

		let new_tcx = Tcx {
			curated_group_id,
			tcx_type,
			content_hash,
		};
		<Tcxs<T>>::insert(tcx_id, new_tcx);
		<NextTcxId<T>>::mutate(|id| *id += <T::TcxId as One>::one());
		<TcxOwner<T>>::insert(tcx_id, curated_group_id);
		<OwnedTcxs<T>>::insert((curated_group_id, owned_tcx_id), tcx_id);
		<NextOwnedTcxId<T>>::mutate(curated_group_id, |id| *id += <T::TcxId as One>::one());

		Self::deposit_event(RawEvent::Created(curated_group_id, tcx_id, tcx_type, content_hash));

        Ok(())
    }

    #[weight = 100_000]
    pub fn propose(origin, tcx_id: T::TcxId, creation_id: <T as creation::Trait>::CreationId, amount: BalanceOf<T>,
    action_id: T::ActionId) -> Result<(), DispatchError> {

      let who = ensure_signed(origin)?;

      // check if creation exists
      let creation_owner = <creation::Module<T>>::owner_of(creation_id).ok_or("Opus does not exist");

      // only member of curated group can propose
      let curated_group_id = Self::owner_of(tcx_id).ok_or("TCX does not exist / TCX owner does not exist")?;
      let curated_group = <curated_group::Module<T>>::curated_groups(curated_group_id).ok_or("Curated group does not exist")?;
      ensure!(<curated_group::Module<T>>::is_member_of_curated_group(curated_group_id, who.clone()), "only member of curated group can propose");

      // TODO: deduction balace for application
      <T as self::Trait>::Currency::reserve(&who, amount)
        .map_err(|_| "proposer's balance too low")?;

      // more than min deposit
      let min_deposit = T::ConvertBalance::convert(curated_group.min_deposit);
      // TODO: quota instead of amount
      ensure!(amount >= min_deposit, "deposit should be more than min_deposit");

      let now = <timestamp::Module<T>>::get();
      let apply_stage_len = curated_group.apply_stage_len;
      let app_exp = now.checked_add(&apply_stage_len).ok_or("Overflow when setting application expiry.")?;

      let listing_id = <NextTcxListingId<T>>::get(tcx_id);

      ensure!(!<TcxListings<T>>::contains_key((tcx_id, creation_id)), "Listing already exists");

      // calculate propose quota
      let quota = match Self::calculate_quota(who.clone(), curated_group_id, amount) {
        Ok(quota) => quota,
        Err(e) => return Err(DispatchError::Other(e)),
      };

      // create a new listing instance
      let new_listing = Listing {
        id: listing_id,
        creation_id: creation_id,
        amount: amount,
        quota: quota,
        whitelisted: false,
        challenge_id: T::ChallengeId::from(0),
        application_expiry: app_exp,
        owner: who.clone(),
      };

      <TcxListings<T>>::insert((tcx_id, creation_id), new_listing);
	    <NextTcxListingId<T>>::mutate(tcx_id, |id| *id += <T::ListingId as One>::one());
      <TcxListingIndexOpus<T>>::insert((tcx_id, listing_id), creation_id);

      Self::deposit_event(RawEvent::Proposed(who, tcx_id, creation_id, app_exp));

      Ok(())
    }

    // TODO: creation_id or listing_id; prevent multiple challenge
    #[weight = 100_000]
    pub fn challenge(origin, tcx_id: T::TcxId, creation_id: <T as creation::Trait>::CreationId, amount: BalanceOf<T>) -> Result<(), DispatchError> {
      let who = ensure_signed(origin)?;

      let curated_group_id = Self::owner_of(tcx_id).ok_or("TCX does not exist / TCX owner does not exist")?;
      let curated_group = <curated_group::Module<T>>::curated_groups(curated_group_id).ok_or("Curated group does not exist")?;

      ensure!(<curated_group::Module<T>>::is_member_of_curated_group(curated_group_id, who.clone()), "only member of curated_group can challenge");

      ensure!(<TcxListings<T>>::contains_key((tcx_id, creation_id)), "Listing not found");

      let listing = Self::tcx_listings((tcx_id, creation_id));

      let quota = match Self::calculate_quota(who.clone(), curated_group_id, amount) {
        Ok(quota) => quota,
        Err(e) => return Err(DispatchError::Other(e)),
      };
      // check if challengable
      ensure!(listing.challenge_id == T::ChallengeId::from(0), "Listing is already challenged.");
      ensure!(listing.owner != who.clone(), "You cannot challenge your own listing.");
      ensure!(quota >= listing.quota, "Quota not enough to challenge");

      let now = <timestamp::Module<T>>::get();
      // check if passed apply stage
      ensure!(listing.application_expiry > now, "Apply stage length has passed.");

      let commit_stage_len = curated_group.commit_stage_len;
      let voting_exp = now.checked_add(&commit_stage_len).ok_or("Overflow when setting voting expiry.")?;

      let new_challenge = Challenge {
        amount,
        quota: quota,
        voting_ends: voting_exp,
        resolved: false,
        reward_pool: BalanceOf::<T>::from(0),
        total_tokens: BalanceOf::<T>::from(0),
        owner: who.clone(),
        tcx_id
      };

      let new_poll = Poll {
        votes_for: listing.amount,
        quota_for: listing.quota,
        votes_against: amount,
        quota_against: quota,
        passed: false,
      };

      // check enough balance, lock it
      // TODO: <token::Module<T>>::lock(sender.clone(), deposit, listing_hash)?;
      <T as self::Trait>::Currency::reserve(&who, amount)
        .map_err(|_| "challenger's balance too low")?;

      let challenge_nonce = <ChallengeNonce<T>>::get();
      let new_challenge_nonce = challenge_nonce.checked_add(&T::ChallengeId::from(1)).ok_or("Exceed maximum challenge count")?;

      // add a new challenge and the corresponding poll
      <Challenges<T>>::insert(new_challenge_nonce, new_challenge);
      <Polls<T>>::insert(new_challenge_nonce, new_poll);

      // update listing with challenge id
      <TcxListings<T>>::mutate((tcx_id, creation_id), |listing| {
        listing.challenge_id = new_challenge_nonce;
      });

      <ChallengeNonce<T>>::put(new_challenge_nonce);

      Self::deposit_event(RawEvent::Challenged(who, new_challenge_nonce, tcx_id, creation_id, voting_exp));

      Ok(())
    }

    // TODO: prevent double votes, cannot vote on your own challenge?
    #[weight = 100_000]
    pub fn vote(origin, challenge_id: T::ChallengeId, amount: BalanceOf<T>, value: bool) -> Result<(), DispatchError> {
      let who = ensure_signed(origin)?;

      // check if listing is challenged
      ensure!(<Challenges<T>>::contains_key(challenge_id), "Challenge does not exist.");
      let challenge = Self::challenges(challenge_id);
      ensure!(challenge.resolved == false, "Challenge is already resolved.");

      // check commit stage length not passed
      let now = <timestamp::Module<T>>::get();
      ensure!(challenge.voting_ends > now, "Commit stage length has passed.");

      // deduct the deposit for vote
      // TODO: <token::Module<T>>::lock(sender.clone(), deposit, challenge.listing_hash)?;
      let mut tmp: [u8; 8] = [0; 8];
      let mutarr = &mut tmp[..];
      let mut moment_num: u64 = challenge.tcx_id.saturated_into::<u64>();
      for i in (0..8).rev() {
        mutarr[i] = (moment_num % 0xff).try_into().unwrap();
        moment_num >>= 8;
      }
      let staking_id = LockIdentifier::from(tmp);

      <T as self::Trait>::Currency::set_lock(
        staking_id,
        &who,
        amount,
        WithdrawReasons::all(),
      );

      // calculate propose quota
      let curated_group_id = Self::owner_of(challenge.tcx_id).ok_or("Cannot find curated_group of tcx")?;
      ensure!(<curated_group::Module<T>>::is_member_of_curated_group(curated_group_id, who.clone()), "only member of curated_group can vote");

      let quota = match Self::calculate_quota(who.clone(), curated_group_id, amount) {
        Ok(quota) => quota,
        Err(e) => return Err(DispatchError::Other(e)),
      };

      let mut poll_instance = Self::polls(challenge_id);
      // based on vote value, increase the count of votes (for or against)
      match value {
        true => {
          poll_instance.votes_for += amount;
          poll_instance.quota_for += quota;
        },
        false => {
          poll_instance.votes_against += amount;
          poll_instance.quota_against += quota;
        },
      }

      // create a new vote instance with the input params
      let vote_instance = Vote {
        value,
        amount,
        quota,
        claimed: false,
      };

      // mutate polls collection to update the poll instance
      <Polls<T>>::mutate(challenge_id, |poll| *poll = poll_instance);

      <Votes<T>>::insert((challenge_id, who.clone()), vote_instance);

      Self::deposit_event(RawEvent::Voted(who, challenge_id, value));
      Ok(())
    }

    #[weight = 100_000]
    pub fn resolve(origin, tcx_id: T::TcxId, creation_id: <T as creation::Trait>::CreationId) -> DispatchResult {
      let who = ensure_signed(origin)?;
      ensure!(<TcxListings<T>>::contains_key((tcx_id, creation_id)), "Listing not found");

      let listing = Self::tcx_listings((tcx_id, creation_id));

      let now = <timestamp::Module<T>>::get();

      // check if listing was challenged
      if listing.challenge_id == T::ChallengeId::from(0) {
        // no challenge
        // check if apply stage length has passed
        ensure!(listing.application_expiry < now, "Apply stage length has not passed.");

        // update listing status
        <TcxListings<T>>::mutate((tcx_id, creation_id), |listing| {
          listing.whitelisted = true;
        });

        Self::deposit_event(RawEvent::Accepted(tcx_id, creation_id));
        return Ok(());
      }

      // listing was challenged
      let challenge = Self::challenges(listing.challenge_id);
      let poll = Self::polls(listing.challenge_id);

      // check commit stage length has passed
      ensure!(challenge.voting_ends < now, "Commit stage length has not passed.");

      let mut whitelisted = false;

      // update the poll instance
      <Polls<T>>::mutate(listing.challenge_id, |poll| {
        if poll.quota_for >= poll.quota_against {
            poll.passed = true;
            whitelisted = true;
        } else {
            poll.passed = false;
        }
      });

      // update listing status
      <TcxListings<T>>::mutate((tcx_id, creation_id), |listing| {
        listing.whitelisted = whitelisted;
        listing.challenge_id = T::ChallengeId::from(0);
      });

      // update challenge
      <Challenges<T>>::mutate(listing.challenge_id, |challenge| {
        challenge.resolved = true;
        if whitelisted == true {
          challenge.total_tokens = poll.votes_for;
          challenge.reward_pool = challenge.amount + poll.votes_against;
        } else {
          challenge.total_tokens = poll.votes_against;
          challenge.reward_pool = listing.amount + poll.votes_for;
        }
      });

      // raise appropriate event as per whitelisting status
      if whitelisted == true {
        Self::deposit_event(RawEvent::Accepted(tcx_id, creation_id));
      } else {
        // if rejected, give challenge deposit back to the challenger
        // TODO: <token::Module<T>>::unlock(challenge.owner, challenge.deposit, listing_hash)?;
		// <T as self::Trait>::Currency::remove_lock(STAKING_ID, &who);
        let amount = <T as self::Trait>::Currency::reserved_balance(&who);
        <T as self::Trait>::Currency::unreserve(&who, amount);
        Self::deposit_event(RawEvent::Rejected(tcx_id, creation_id));
      }

      Self::deposit_event(RawEvent::Resolved(listing.challenge_id));
      Ok(())
    }

    #[weight = 100_000]
    pub fn claim(origin, challenge_id: T::ChallengeId) -> DispatchResult {
      let who = ensure_signed(origin)?;

      ensure!(<Challenges<T>>::contains_key(challenge_id), "Challenge not found.");
      let challenge = Self::challenges(challenge_id);
      ensure!(challenge.resolved == true, "Challenge is not resolved.");

      // reward depends on poll passed status and vote value
      let poll = Self::polls(challenge_id);
      let vote = Self::votes((challenge_id, who.clone()));

      // ensure vote reward is not already claimed
      ensure!(vote.claimed == false, "Vote reward has already been claimed.");

      // if winning party, calculate reward and transfer
      if poll.passed == vote.value {
        // TODO: claim reward
        // let reward_ratio = challenge.reward_pool.checked_div(&challenge.total_tokens).ok_or("overflow in calculating reward")?;
        // let reward = reward_ratio.checked_mul(&vote.deposit).ok_or("overflow in calculating reward")?;
        // let total = reward.checked_add(&vote.deposit).ok_or("overflow in calculating reward")?;
        // <token::Module<T>>::unlock(sender.clone(), total, challenge.listing_hash)?;

        let mut tmp: [u8; 8] = [0; 8];
        let mutarr = &mut tmp[..];
        let mut moment_num: u64 = challenge.tcx_id.saturated_into::<u64>();
        for i in (0..8).rev() {
          mutarr[i] = (moment_num % 0xff).try_into().unwrap();
          moment_num >>= 8;
        }
        let staking_id = LockIdentifier::from(tmp);
        <T as self::Trait>::Currency::remove_lock(staking_id, &who);

        Self::deposit_event(RawEvent::Claimed(who.clone(), challenge_id));
      }

      // update vote reward claimed status
      <Votes<T>>::mutate((challenge_id, who), |vote| vote.claimed = true);

      Ok(())
    }
  }
}

decl_event!(
  pub enum Event<T>
  where
    AccountId = <T as system::Trait>::AccountId,
    CreationId = <T as creation::Trait>::CreationId,
    TcxId = <T as Trait>::TcxId,
    TcxType = <T as Trait>::TcxType,
    ChallengeId = <T as Trait>::ChallengeId,
    CuratedGroupId = <T as curated_group::Trait>::CuratedGroupId,
    ContentHash = <T as curated_group::Trait>::ContentHash,
    Moment = <T as timestamp::Trait>::Moment
  {
    Proposed(AccountId, TcxId, CreationId, Moment),
    Challenged(AccountId, ChallengeId, TcxId, CreationId, Moment),
    Voted(AccountId, ChallengeId, bool),
    Resolved(ChallengeId),
    Accepted(TcxId, CreationId),
    Rejected(TcxId, CreationId),
    Claimed(AccountId, ChallengeId),
    Created(CuratedGroupId, TcxId, TcxType, ContentHash),
  }
);

impl<T: Trait> Module<T> {
	pub fn calculate_quota(who: T::AccountId, curated_group_id: T::CuratedGroupId, amount: BalanceOf<T>) -> result::Result<BalanceOf<T>, &'static str> {
		// calculate propose quota
		let invested = T::ConvertBalance::convert(<curated_group::Module<T>>::invested_amount((curated_group_id, who.clone())));
		let min = cmp::min(amount, invested);
		let factor = BalanceOf::<T>::from(20);
		let quota = min.checked_mul(&factor).ok_or("Overflow calculating A shares.")?;
		let staked = T::ConvertBalance::convert(<curated_group::Module<T>>::staked_amount((curated_group_id, who.clone())));
		// let max = cmp::max(BalanceOf::<T>::from(0), amount-invested);
		let max = if amount > invested {
			amount - invested
		} else {
			BalanceOf::<T>::from(0)
		};
		let max = cmp::max(max, staked);
		let quota = quota.checked_add(&max).ok_or("Overflow calculating B shares.")?;
		Ok(quota)
	}
}
