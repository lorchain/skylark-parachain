#![cfg_attr(not(feature = "std"), no_std)]

use frame_support::{
    decl_module, decl_storage, decl_event, StorageValue, StorageMap, Parameter, ensure,
    dispatch::DispatchResult,
    weights::{Weight, DispatchClass},
};
use sp_runtime::traits::{ Member };
use frame_system::{self as system, ensure_signed};
use codec::{Encode, Decode};
use sp_std::prelude::*;

/// The module's configuration trait.
pub trait Trait: system::Trait {
    /// The overarching event type.
    type Event: From<Event<Self>> + Into<<Self as system::Trait>::Event>;
    type CreationId: Parameter + Member + Default + Copy;
    type CreationType: Parameter + Member + Default + Copy;
    type Topic: Parameter + Member + Default + Copy;
}

#[cfg_attr(feature ="std", derive(Debug))]
#[derive(Encode, Decode, Default, Clone, PartialEq)]
pub struct Creation<CreationId, CreationType, Topic> {
    pub creation_type: CreationType,
    pub topic: Topic,
    pub sources: Vec<CreationId>
}

// This module's storage items.
decl_storage! {
  trait Store for Module<T: Trait> as Creation {
    Creations get(fn creations): map hasher(blake2_128_concat) T::CreationId => Option<Creation<T::CreationId, T::CreationType, T::Topic>>;
    CreationOwner get(fn owner_of): map hasher(blake2_128_concat) T::CreationId => Option<T::AccountId>;

    IndexCreation get(fn creation_by_index): map hasher(blake2_128_concat) u64 => T::CreationId;
    CreationCount get(fn creation_count): u64;
    CreationIndex: map hasher(blake2_128_concat) T::CreationId => u64;

    OwnedIndexCreation get(fn owned_creation_by_index): map hasher(blake2_128_concat) (T::AccountId, u64) => T::CreationId;
    OwnedCreationCount get(fn owned_creation_count): map hasher(blake2_128_concat) T::AccountId => u64;
    OwnedCreationIndex: map hasher(blake2_128_concat) T::CreationId => u64;
  }
}

// The module's dispatchable functions.
decl_module! {
  /// The module declaration.
  pub struct Module<T: Trait> for enum Call where origin: T::Origin {
    // Initializing events.
    fn deposit_event() = default;

    #[weight = 100_000]
    pub fn create(origin, creation_id: T::CreationId, creation_type: T::CreationType, topic: T::Topic, sources: Vec<T::CreationId>) -> DispatchResult {
      let sender = ensure_signed(origin)?;

      ensure!(!<CreationOwner<T>>::contains_key(creation_id), "Creation already exists");
      ensure!(sources.len() <= 10, "Cannot link more than 10 sources");

      let new_creation = Creation {
          creation_type,
          topic,
          sources: sources.clone(),
      };

      let new_creation_count = Self::creation_count() + 1;

      let new_owned_creation_count = Self::owned_creation_count(sender.clone()) + 1;

      <Creations<T>>::insert(creation_id, new_creation);
      <CreationOwner<T>>::insert(creation_id, sender.clone());

      <IndexCreation<T>>::insert(new_creation_count, creation_id);
      CreationCount::put(new_creation_count);
      <CreationIndex<T>>::insert(creation_id, new_creation_count);

      <OwnedIndexCreation<T>>::insert((sender.clone(), new_owned_creation_count), creation_id);
      <OwnedCreationCount<T>>::insert(sender.clone(), new_owned_creation_count);
      <OwnedCreationIndex<T>>::insert(creation_id, new_owned_creation_count);

      Self::deposit_event(RawEvent::Created(sender, creation_id, creation_type, topic, sources));

      Ok(())
    }

    #[weight = 100_000]
    pub fn transfer(origin, to: T::AccountId, creation_id: T::CreationId) -> DispatchResult {
      let sender = ensure_signed(origin)?;

      let owner = Self::owner_of(creation_id).ok_or("No creation owner")?;

      ensure!(owner == sender.clone(), "Sender does not own the creation");

      let owned_creation_count_from = Self::owned_creation_count(sender.clone());
      let owned_creation_count_to = Self::owned_creation_count(to.clone());

      let new_owned_creation_count_to = owned_creation_count_to.checked_add(1)
        .ok_or("Transfer causes overflow for creation receiver")?;

      let new_owned_creation_count_from = owned_creation_count_from.checked_sub(1)
        .ok_or("Transfer causes underflow for creation sender")?;

      let owned_creation_index = <OwnedCreationIndex<T>>::get(creation_id);
      if owned_creation_index != new_owned_creation_count_from {
        let last_owned_creation_id = <OwnedIndexCreation<T>>::get((sender.clone(), new_owned_creation_count_from));
        <OwnedIndexCreation<T>>::insert((sender.clone(), owned_creation_index), last_owned_creation_id);
        <OwnedCreationIndex<T>>::insert(last_owned_creation_id, owned_creation_index);
      }

      <CreationOwner<T>>::insert(creation_id, to.clone());
      <OwnedCreationIndex<T>>::insert(creation_id, owned_creation_count_to);

      <OwnedIndexCreation<T>>::remove((sender.clone(), new_owned_creation_count_from));
      <OwnedIndexCreation<T>>::insert((to.clone(), owned_creation_count_to), creation_id);

      <OwnedCreationCount<T>>::insert(sender.clone(), new_owned_creation_count_from);
      <OwnedCreationCount<T>>::insert(to.clone(), new_owned_creation_count_to);

      Self::deposit_event(RawEvent::Transferred(sender, to, creation_id));

      Ok(())
    }

  }
}

decl_event!(
  pub enum Event<T>
  where
    AccountId = <T as system::Trait>::AccountId,
    CreationId = <T as Trait>::CreationId,
    CreationType = <T as Trait>::CreationType,
    Topic = <T as Trait>::Topic,
    VecCreationId = Vec<<T as Trait>::CreationId>,
  {
    Created(AccountId, CreationId, CreationType, Topic, VecCreationId),
    Transferred(AccountId, AccountId, CreationId),
  }
);
