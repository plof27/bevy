use std::marker::PhantomData;

use bevy_utils::{tracing::warn, HashSet};

use crate::{component::ComponentId, prelude::Bundle, world::World};

/// A rule about which [`Component`]s can coexist on entities
///
/// These rules must be true at all times for all entities in the [`World`].
///
/// When added to the [`World`], archetype invariants behave like [`assert!`];
/// all archetype invariants must be true for every entity in the [`World`].
/// Archetype invariants are checked each time [`Archetypes`] is modified;
/// this can occur on component addition, component removal, and entity spawning.
///
/// Archetypes are only modified when a novel archetype (set of components) is seen for the first time;
/// swapping between existing archetypes will not trigger these checks.
///
/// Note that this is converted to an [`UntypedArchetypeInvariant`] when added to a [`World`].
/// This is to ensure compatibility between different invariants.
#[derive(Clone, Debug, PartialEq)]
pub struct ArchetypeInvariant<B1: Bundle, B2: Bundle = B1> {
    /// For all entities where the predicate is true
    pub predicate: ArchetypeStatement<B1>,
    /// The consequence must also be true
    pub consequence: ArchetypeStatement<B2>,
}

impl<B1: Bundle, B2: Bundle> ArchetypeInvariant<B1, B2> {
    /// Erases the type information of this archetype invariant.
    ///
    /// Requires mutable world access, since the components might not have been added to the world yet.
    #[inline]
    pub fn into_untyped(self, world: &mut World) -> UntypedArchetypeInvariant {
        UntypedArchetypeInvariant {
            predicate: self.predicate.into_untyped(world),
            consequence: self.consequence.into_untyped(world),
        }
    }
}

impl<B: Bundle> ArchetypeInvariant<B, B> {
    /// This is a helper function for constructing common invariants.
    /// All components of the provided bundle require each other.
    /// In other words, if any one component of this bundle is present, then all of them must be.
    #[inline]
    pub fn full_bundle() -> Self {
        Self {
            predicate: ArchetypeStatement::<B>::at_least_one_of(),
            consequence: ArchetypeStatement::<B>::all_of(),
        }
    }
}

/// A statement about the presence or absence of some subset of components in the given [`Bundle`]
///
/// This type is used as part of an [`ArchetypeInvariant`].
///
/// When used as a predicate, the archetype invariant matches all entities which satisfy the statement.
/// When used as a consquence, then the statment must be true for all entities that were matched by the predicate.
///
/// For the statements about a single component `C`, wrap it in a single-component bundle `(C,)`.
/// For single component bundles, `AllOf` and `AtLeastOneOf` are equivalent.
/// Prefer `ArchetypeStatement::<(C,)>::all_of` over `ArchetypeStatement::<(C,)>::at_least_one_of` for consistency and clarity.
///
/// Note that this is converted to an [`UntypedArchetypeStatment`] when added to a [`World`].
/// This is to ensure compatibility between different invariants.
#[derive(Clone, Debug, PartialEq)]
pub enum ArchetypeStatement<B: Bundle> {
    /// Evaluates to true if and only if the entity has all of the components present in the bundle `B`
    AllOf(PhantomData<B>),
    /// The entity has at least one component in the bundle `B`, and may have all of them.
    /// When using a single-component bundle, `AllOf` is preferred.
    AtLeastOneOf(PhantomData<B>),
    /// The entity has none of the components in the bundle `B`
    NoneOf(PhantomData<B>),
}

impl<B: Bundle> ArchetypeStatement<B> {
    /// Erases the type information of this archetype statment.
    ///
    /// Requires mutable world access, since the components might not have been added to the world yet.
    pub fn into_untyped(self, world: &mut World) -> UntypedArchetypeStatement {
        let component_ids = B::component_ids(&mut world.components, &mut world.storages);
        let component_ids: HashSet<ComponentId> = component_ids.into_iter().collect();

        match self {
            ArchetypeStatement::AllOf(_) => UntypedArchetypeStatement::AllOf(component_ids),
            ArchetypeStatement::AtLeastOneOf(_) => {
                if component_ids.len() == 1 {
                    warn!("An `ArchetypeStatement::AtLeastOneOf` was constructed for a bundle with only one component. Prefer the equivalent `ArchetypeStatment:AllOf` for consistency and clarity.");
                }
                UntypedArchetypeStatement::AtLeastOneOf(component_ids)
            }
            ArchetypeStatement::NoneOf(_) => UntypedArchetypeStatement::NoneOf(component_ids),
        }
    }

    /// Constructs a new [`ArchetypeStatement::AllOf`] variant for all components stored in the bundle `B`
    #[inline]
    pub const fn all_of() -> Self {
        ArchetypeStatement::AllOf(PhantomData)
    }

    /// Constructs a new [`ArchetypeStatement::AtLeastOneOf`] variant for all components stored in the bundle `B`
    #[inline]
    pub const fn at_least_one_of() -> Self {
        ArchetypeStatement::AtLeastOneOf(PhantomData)
    }

    /// Constructs a new [`ArchetypeStatement::NoneOf`] variant for all components stored in the bundle `B`
    #[inline]
    pub const fn none_of() -> Self {
        ArchetypeStatement::NoneOf(PhantomData)
    }
}

/// A type-erased version of [`ArchetypeInvariant`].
/// Intended to be used with dynamic components that cannot be represented with Rust types.
/// Prefer [`ArchetypeInvariant`] when possible.
#[derive(Clone, Debug, PartialEq)]
pub struct UntypedArchetypeInvariant {
    /// For all entities where the predicate is true
    pub predicate: UntypedArchetypeStatement,
    /// The consequence must also be true
    pub consequence: UntypedArchetypeStatement,
}

/// A type-erased version of [`ArchetypeStatment`]
/// Intended to be used with dynamic components that cannot be represented with Rust types.
/// Prefer [`ArchetypeStatment`] when possible.
#[derive(Clone, Debug, PartialEq)]
pub enum UntypedArchetypeStatement {
    /// Evaluates to true if and only if the entity has all of the components present in the set
    AllOf(HashSet<ComponentId>),
    /// The entity has at least one component in the set, and may have all of them.
    /// When using a single-component bundle, `AllOf` is preferred
    AtLeastOneOf(HashSet<ComponentId>),
    /// The entity has none of the components in the set
    NoneOf(HashSet<ComponentId>),
}

#[derive(Default)]
pub struct ArchetypeInvariants {
    raw_list: Vec<UntypedArchetypeInvariant>,
    last_checked_archetype_index: u32,
}

impl ArchetypeInvariants {
    /// Adds a new [`ArchetypeInvariant`] to this set of archetype invariants.
    ///
    /// Whenever a new archetype invariant is added, all existing archetypes are re-checked.
    /// This may include empty archetypes- archetypes that contain no entities.
    pub fn add(&mut self, archetype_invariant: UntypedArchetypeInvariant) {
        self.last_checked_archetype_index = 0;
        self.raw_list.push(archetype_invariant);
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        self as bevy_ecs, component::Component, world::archetype_invariants::ArchetypeInvariant,
        world::World,
    };

    #[derive(Component)]
    struct A;

    #[derive(Component)]
    struct B;

    #[derive(Component)]
    struct C;

    #[test]
    fn full_bundle() {
        let mut world = World::new();

        world.add_archetype_invariant(ArchetypeInvariant::<(A, B, C)>::full_bundle());

        todo!();
    }
}
