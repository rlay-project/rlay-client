//! Extends the `rlay_ontology` by a few structs and traits that are helpful in the context of
//! aggregation.

use cid::{self, Cid, ToCid};
use rlay_ontology::ontology;

#[derive(Debug, Clone, PartialEq)]
pub enum Assertion {
    ClassAssertion(ontology::ClassAssertion),
    NegativeClassAssertion(ontology::NegativeClassAssertion),
    DataPropertyAssertion(ontology::DataPropertyAssertion),
    NegativeDataPropertyAssertion(ontology::NegativeDataPropertyAssertion),
    ObjectPropertyAssertion(ontology::ObjectPropertyAssertion),
    NegativeObjectPropertyAssertion(ontology::NegativeObjectPropertyAssertion),
}

impl ToCid for Assertion {
    fn to_cid(&self) -> Result<Cid, cid::Error> {
        match self {
            Assertion::ClassAssertion(val) => val.to_cid(),
            Assertion::NegativeClassAssertion(val) => val.to_cid(),
            Assertion::DataPropertyAssertion(val) => val.to_cid(),
            Assertion::NegativeDataPropertyAssertion(val) => val.to_cid(),
            Assertion::ObjectPropertyAssertion(val) => val.to_cid(),
            Assertion::NegativeObjectPropertyAssertion(val) => val.to_cid(),
        }
    }
}

impl Into<ontology::Entity> for Assertion {
    fn into(self) -> ontology::Entity {
        match self {
            Assertion::ClassAssertion(val) => ontology::Entity::ClassAssertion(val),
            Assertion::NegativeClassAssertion(val) => ontology::Entity::NegativeClassAssertion(val),
            Assertion::DataPropertyAssertion(val) => ontology::Entity::DataPropertyAssertion(val),
            Assertion::NegativeDataPropertyAssertion(val) => {
                ontology::Entity::NegativeDataPropertyAssertion(val)
            }
            Assertion::ObjectPropertyAssertion(val) => {
                ontology::Entity::ObjectPropertyAssertion(val)
            }
            Assertion::NegativeObjectPropertyAssertion(val) => {
                ontology::Entity::NegativeObjectPropertyAssertion(val)
            }
        }
    }
}

pub trait AsAssertion {
    fn as_assertion(&self) -> Option<Assertion>;
}

impl AsAssertion for ontology::Entity {
    fn as_assertion(&self) -> Option<Assertion> {
        match &self {
            ontology::Entity::ClassAssertion(val) => Some(Assertion::ClassAssertion(val.clone())),
            ontology::Entity::NegativeClassAssertion(val) => {
                Some(Assertion::NegativeClassAssertion(val.clone()))
            }
            ontology::Entity::DataPropertyAssertion(val) => {
                Some(Assertion::DataPropertyAssertion(val.clone()))
            }
            ontology::Entity::NegativeDataPropertyAssertion(val) => {
                Some(Assertion::NegativeDataPropertyAssertion(val.clone()))
            }
            ontology::Entity::ObjectPropertyAssertion(val) => {
                Some(Assertion::ObjectPropertyAssertion(val.clone()))
            }
            ontology::Entity::NegativeObjectPropertyAssertion(val) => {
                Some(Assertion::NegativeObjectPropertyAssertion(val.clone()))
            }
            _ => None,
        }
    }
}

pub trait CanonicalParts {
    fn canonical_parts(&self) -> Vec<Vec<u8>>;
}

impl CanonicalParts for Assertion {
    fn canonical_parts(&self) -> Vec<Vec<u8>> {
        match self {
            Assertion::ClassAssertion(val) => val.canonical_parts(),
            Assertion::NegativeClassAssertion(val) => val.canonical_parts(),
            Assertion::DataPropertyAssertion(val) => val.canonical_parts(),
            Assertion::NegativeDataPropertyAssertion(val) => val.canonical_parts(),
            Assertion::ObjectPropertyAssertion(val) => val.canonical_parts(),
            Assertion::NegativeObjectPropertyAssertion(val) => val.canonical_parts(),
        }
    }
}

impl CanonicalParts for ontology::ClassAssertion {
    fn canonical_parts(&self) -> Vec<Vec<u8>> {
        let mut parts = Vec::new();

        if let Some(ref val) = self.subject {
            parts.push(val.clone());
        }
        parts.push(self.class.clone());

        parts
    }
}

impl CanonicalParts for ontology::NegativeClassAssertion {
    fn canonical_parts(&self) -> Vec<Vec<u8>> {
        let mut parts = Vec::new();

        if let Some(ref val) = self.subject {
            parts.push(val.clone());
        }
        parts.push(self.class.clone());

        parts
    }
}

impl CanonicalParts for ontology::DataPropertyAssertion {
    fn canonical_parts(&self) -> Vec<Vec<u8>> {
        let mut parts = Vec::new();

        if let Some(ref val) = self.subject {
            parts.push(val.clone());
        }
        if let Some(ref val) = self.property {
            parts.push(val.clone());
        }
        if let Some(ref val) = self.target {
            parts.push(val.clone());
        }

        parts
    }
}

impl CanonicalParts for ontology::NegativeDataPropertyAssertion {
    fn canonical_parts(&self) -> Vec<Vec<u8>> {
        let mut parts = Vec::new();

        if let Some(ref val) = self.subject {
            parts.push(val.clone());
        }
        if let Some(ref val) = self.property {
            parts.push(val.clone());
        }
        if let Some(ref val) = self.target {
            parts.push(val.clone());
        }

        parts
    }
}

impl CanonicalParts for ontology::ObjectPropertyAssertion {
    fn canonical_parts(&self) -> Vec<Vec<u8>> {
        let mut parts = Vec::new();

        if let Some(ref val) = self.subject {
            parts.push(val.clone());
        }
        if let Some(ref val) = self.property {
            parts.push(val.clone());
        }
        if let Some(ref val) = self.target {
            parts.push(val.clone());
        }

        parts
    }
}

impl CanonicalParts for ontology::NegativeObjectPropertyAssertion {
    fn canonical_parts(&self) -> Vec<Vec<u8>> {
        let mut parts = Vec::new();

        if let Some(ref val) = self.subject {
            parts.push(val.clone());
        }
        if let Some(ref val) = self.property {
            parts.push(val.clone());
        }
        if let Some(ref val) = self.target {
            parts.push(val.clone());
        }

        parts
    }
}

pub trait GetSubject {
    fn get_subject(&self) -> Option<&[u8]>;
}

impl GetSubject for Assertion {
    fn get_subject(&self) -> Option<&[u8]> {
        match self {
            Assertion::ClassAssertion(val) => GetSubject::get_subject(val),
            Assertion::NegativeClassAssertion(val) => GetSubject::get_subject(val),
            Assertion::DataPropertyAssertion(val) => GetSubject::get_subject(val),
            Assertion::NegativeDataPropertyAssertion(val) => GetSubject::get_subject(val),
            Assertion::ObjectPropertyAssertion(val) => GetSubject::get_subject(val),
            Assertion::NegativeObjectPropertyAssertion(val) => GetSubject::get_subject(val),
        }
    }
}

impl GetSubject for ontology::ClassAssertion {
    fn get_subject(&self) -> Option<&[u8]> {
        self.subject.as_ref().map(|n| n.as_slice())
    }
}

impl GetSubject for ontology::NegativeClassAssertion {
    fn get_subject(&self) -> Option<&[u8]> {
        self.subject.as_ref().map(|n| n.as_slice())
    }
}

impl GetSubject for ontology::DataPropertyAssertion {
    fn get_subject(&self) -> Option<&[u8]> {
        self.subject.as_ref().map(|n| n.as_slice())
    }
}

impl GetSubject for ontology::NegativeDataPropertyAssertion {
    fn get_subject(&self) -> Option<&[u8]> {
        self.subject.as_ref().map(|n| n.as_slice())
    }
}

impl GetSubject for ontology::ObjectPropertyAssertion {
    fn get_subject(&self) -> Option<&[u8]> {
        self.subject.as_ref().map(|n| n.as_slice())
    }
}

impl GetSubject for ontology::NegativeObjectPropertyAssertion {
    fn get_subject(&self) -> Option<&[u8]> {
        self.subject.as_ref().map(|n| n.as_slice())
    }
}

pub trait IsPositiveAssertion {
    fn is_positive(&self) -> bool;
}

impl IsPositiveAssertion for Assertion {
    fn is_positive(&self) -> bool {
        match self {
            Assertion::ClassAssertion(val) => IsPositiveAssertion::is_positive(val),
            Assertion::NegativeClassAssertion(val) => IsPositiveAssertion::is_positive(val),
            Assertion::DataPropertyAssertion(val) => IsPositiveAssertion::is_positive(val),
            Assertion::NegativeDataPropertyAssertion(val) => IsPositiveAssertion::is_positive(val),
            Assertion::ObjectPropertyAssertion(val) => IsPositiveAssertion::is_positive(val),
            Assertion::NegativeObjectPropertyAssertion(val) => {
                IsPositiveAssertion::is_positive(val)
            }
        }
    }
}

impl IsPositiveAssertion for ontology::ClassAssertion {
    fn is_positive(&self) -> bool {
        true
    }
}

impl IsPositiveAssertion for ontology::NegativeClassAssertion {
    fn is_positive(&self) -> bool {
        false
    }
}

impl IsPositiveAssertion for ontology::DataPropertyAssertion {
    fn is_positive(&self) -> bool {
        true
    }
}

impl IsPositiveAssertion for ontology::NegativeDataPropertyAssertion {
    fn is_positive(&self) -> bool {
        false
    }
}

impl IsPositiveAssertion for ontology::ObjectPropertyAssertion {
    fn is_positive(&self) -> bool {
        true
    }
}

impl IsPositiveAssertion for ontology::NegativeObjectPropertyAssertion {
    fn is_positive(&self) -> bool {
        false
    }
}

pub trait IsNegativeAssertion: IsPositiveAssertion {
    fn is_negative(&self) -> bool {
        !self.is_positive()
    }
}

impl<T> IsNegativeAssertion for T
where
    T: IsPositiveAssertion,
{
    fn is_negative(&self) -> bool {
        !self.is_positive()
    }
}

/// Allows an assertion to be turned into a "canonical" assertion.
///
/// Every assertion can be reduced to its canonical assertion by removing all annotations and other
/// fields without semantic meaning.
///
/// - For a `ClassAssertion`/`NegativeClassAssertion`, the remaining fields are `subject`, `class`
///
/// - For a `DataPropertyAssertion`/`NegativeDataPropertyAssertion`
///   and `ObjectPropertyAssertion`/`NegativeObjectPropertyAssertion`,
///   the remaining fields are `subject`, `property`, `target`
pub trait CanonicalAssertion {
    fn canonical_assertion(&self) -> Self;
}

impl CanonicalAssertion for Assertion {
    fn canonical_assertion(&self) -> Self {
        match self {
            Assertion::ClassAssertion(val) => Assertion::ClassAssertion(val.canonical_assertion()),
            Assertion::NegativeClassAssertion(val) => {
                Assertion::NegativeClassAssertion(val.canonical_assertion())
            }
            Assertion::DataPropertyAssertion(val) => {
                Assertion::DataPropertyAssertion(val.canonical_assertion())
            }
            Assertion::NegativeDataPropertyAssertion(val) => {
                Assertion::NegativeDataPropertyAssertion(val.canonical_assertion())
            }
            Assertion::ObjectPropertyAssertion(val) => {
                Assertion::ObjectPropertyAssertion(val.canonical_assertion())
            }
            Assertion::NegativeObjectPropertyAssertion(val) => {
                Assertion::NegativeObjectPropertyAssertion(val.canonical_assertion())
            }
        }
    }
}

impl CanonicalAssertion for ontology::ClassAssertion {
    fn canonical_assertion(&self) -> Self {
        Self {
            subject: self.subject.clone(),
            class: self.class.clone(),
            ..Self::default()
        }
    }
}

impl CanonicalAssertion for ontology::NegativeClassAssertion {
    fn canonical_assertion(&self) -> Self {
        Self {
            subject: self.subject.clone(),
            class: self.class.clone(),
            ..Self::default()
        }
    }
}

impl CanonicalAssertion for ontology::DataPropertyAssertion {
    fn canonical_assertion(&self) -> Self {
        Self {
            subject: self.subject.clone(),
            property: self.property.clone(),
            target: self.target.clone(),
            ..Self::default()
        }
    }
}

impl CanonicalAssertion for ontology::NegativeDataPropertyAssertion {
    fn canonical_assertion(&self) -> Self {
        Self {
            subject: self.subject.clone(),
            property: self.property.clone(),
            target: self.target.clone(),
            ..Self::default()
        }
    }
}

impl CanonicalAssertion for ontology::ObjectPropertyAssertion {
    fn canonical_assertion(&self) -> Self {
        Self {
            subject: self.subject.clone(),
            property: self.property.clone(),
            target: self.target.clone(),
            ..Self::default()
        }
    }
}

impl CanonicalAssertion for ontology::NegativeObjectPropertyAssertion {
    fn canonical_assertion(&self) -> Self {
        Self {
            subject: self.subject.clone(),
            property: self.property.clone(),
            target: self.target.clone(),
            ..Self::default()
        }
    }
}

pub trait CanonicalOppositeAssertion {
    /// The entity type for a opposing assertion.
    type OppositeAssertion;

    fn canonical_opposite_assertion(&self) -> Self::OppositeAssertion;
}

impl CanonicalOppositeAssertion for Assertion {
    type OppositeAssertion = Self;

    fn canonical_opposite_assertion(&self) -> Self::OppositeAssertion {
        match self {
            Assertion::ClassAssertion(val) => {
                Assertion::NegativeClassAssertion(val.canonical_opposite_assertion())
            }
            Assertion::NegativeClassAssertion(val) => {
                Assertion::ClassAssertion(val.canonical_opposite_assertion())
            }
            Assertion::DataPropertyAssertion(val) => {
                Assertion::NegativeDataPropertyAssertion(val.canonical_opposite_assertion())
            }
            Assertion::NegativeDataPropertyAssertion(val) => {
                Assertion::DataPropertyAssertion(val.canonical_opposite_assertion())
            }
            Assertion::ObjectPropertyAssertion(val) => {
                Assertion::NegativeObjectPropertyAssertion(val.canonical_opposite_assertion())
            }
            Assertion::NegativeObjectPropertyAssertion(val) => {
                Assertion::ObjectPropertyAssertion(val.canonical_opposite_assertion())
            }
        }
    }
}

impl CanonicalOppositeAssertion for ontology::ClassAssertion {
    type OppositeAssertion = ontology::NegativeClassAssertion;

    fn canonical_opposite_assertion(&self) -> Self::OppositeAssertion {
        Self::OppositeAssertion {
            subject: self.subject.clone(),
            class: self.class.clone(),
            ..Self::OppositeAssertion::default()
        }
    }
}

impl CanonicalOppositeAssertion for ontology::NegativeClassAssertion {
    type OppositeAssertion = ontology::ClassAssertion;

    fn canonical_opposite_assertion(&self) -> Self::OppositeAssertion {
        Self::OppositeAssertion {
            subject: self.subject.clone(),
            class: self.class.clone(),
            ..Self::OppositeAssertion::default()
        }
    }
}

impl CanonicalOppositeAssertion for ontology::DataPropertyAssertion {
    type OppositeAssertion = ontology::NegativeDataPropertyAssertion;

    fn canonical_opposite_assertion(&self) -> Self::OppositeAssertion {
        Self::OppositeAssertion {
            subject: self.subject.clone(),
            property: self.property.clone(),
            target: self.target.clone(),
            ..Self::OppositeAssertion::default()
        }
    }
}

impl CanonicalOppositeAssertion for ontology::NegativeDataPropertyAssertion {
    type OppositeAssertion = ontology::DataPropertyAssertion;

    fn canonical_opposite_assertion(&self) -> Self::OppositeAssertion {
        Self::OppositeAssertion {
            subject: self.subject.clone(),
            property: self.property.clone(),
            target: self.target.clone(),
            ..Self::OppositeAssertion::default()
        }
    }
}

impl CanonicalOppositeAssertion for ontology::ObjectPropertyAssertion {
    type OppositeAssertion = ontology::NegativeObjectPropertyAssertion;

    fn canonical_opposite_assertion(&self) -> Self::OppositeAssertion {
        Self::OppositeAssertion {
            subject: self.subject.clone(),
            property: self.property.clone(),
            target: self.target.clone(),
            ..Self::OppositeAssertion::default()
        }
    }
}

impl CanonicalOppositeAssertion for ontology::NegativeObjectPropertyAssertion {
    type OppositeAssertion = ontology::ObjectPropertyAssertion;

    fn canonical_opposite_assertion(&self) -> Self::OppositeAssertion {
        Self::OppositeAssertion {
            subject: self.subject.clone(),
            property: self.property.clone(),
            target: self.target.clone(),
            ..Self::OppositeAssertion::default()
        }
    }
}

/// Get the subject-property pair for an Assertion.
///
/// In the case of a `ClassAssertion`/`NegativeClassAssertion`, this is only the `subject`, as they
/// have a implied "is a"-property.
pub trait GetSubjectProperty {
    fn get_subject_property(&self) -> Vec<&[u8]>;
}

impl GetSubjectProperty for Assertion {
    fn get_subject_property(&self) -> Vec<&[u8]> {
        match self {
            Assertion::ClassAssertion(val) => GetSubjectProperty::get_subject_property(val),
            Assertion::NegativeClassAssertion(val) => GetSubjectProperty::get_subject_property(val),
            Assertion::DataPropertyAssertion(val) => GetSubjectProperty::get_subject_property(val),
            Assertion::NegativeDataPropertyAssertion(val) => {
                GetSubjectProperty::get_subject_property(val)
            }
            Assertion::ObjectPropertyAssertion(val) => {
                GetSubjectProperty::get_subject_property(val)
            }
            Assertion::NegativeObjectPropertyAssertion(val) => {
                GetSubjectProperty::get_subject_property(val)
            }
        }
    }
}

impl GetSubjectProperty for ontology::ClassAssertion {
    fn get_subject_property(&self) -> Vec<&[u8]> {
        let mut vals = Vec::new();
        if let Some(ref val) = self.subject {
            vals.push(val.as_slice());
        }
        vals
    }
}

impl GetSubjectProperty for ontology::NegativeClassAssertion {
    fn get_subject_property(&self) -> Vec<&[u8]> {
        let mut vals = Vec::new();
        if let Some(ref val) = self.subject {
            vals.push(val.as_slice());
        }
        vals
    }
}

impl GetSubjectProperty for ontology::DataPropertyAssertion {
    fn get_subject_property(&self) -> Vec<&[u8]> {
        let mut vals = Vec::new();
        if let Some(ref val) = self.subject {
            vals.push(val.as_slice());
        }
        if let Some(ref val) = self.property {
            vals.push(val.as_slice());
        }
        vals
    }
}

impl GetSubjectProperty for ontology::NegativeDataPropertyAssertion {
    fn get_subject_property(&self) -> Vec<&[u8]> {
        let mut vals = Vec::new();
        if let Some(ref val) = self.subject {
            vals.push(val.as_slice());
        }
        if let Some(ref val) = self.property {
            vals.push(val.as_slice());
        }
        vals
    }
}

impl GetSubjectProperty for ontology::ObjectPropertyAssertion {
    fn get_subject_property(&self) -> Vec<&[u8]> {
        let mut vals = Vec::new();
        if let Some(ref val) = self.subject {
            vals.push(val.as_slice());
        }
        if let Some(ref val) = self.property {
            vals.push(val.as_slice());
        }
        vals
    }
}

impl GetSubjectProperty for ontology::NegativeObjectPropertyAssertion {
    fn get_subject_property(&self) -> Vec<&[u8]> {
        let mut vals = Vec::new();
        if let Some(ref val) = self.subject {
            vals.push(val.as_slice());
        }
        if let Some(ref val) = self.property {
            vals.push(val.as_slice());
        }
        vals
    }
}

/// Get the "target" for an Assertion.
///
/// When viewing an assertion as a Subject-Propery-Object triple, this would be the object.
///
/// - For a `ClassAssertion`/`NegativeClassAssertion`, this is the `class`
///
/// - For a `DataPropertyAssertion`/`NegativeDataPropertyAssertion`, this is the `target` value
///
/// - For a `ObjectPropertyAssertion`/`NegativeObjectPropertyAssertion`, this is the `target`
/// object
pub trait GetTarget {
    fn get_target(&self) -> Option<&[u8]>;
}

impl GetTarget for Assertion {
    fn get_target(&self) -> Option<&[u8]> {
        match self {
            Assertion::ClassAssertion(val) => GetTarget::get_target(val),
            Assertion::NegativeClassAssertion(val) => GetTarget::get_target(val),
            Assertion::DataPropertyAssertion(val) => GetTarget::get_target(val),
            Assertion::NegativeDataPropertyAssertion(val) => GetTarget::get_target(val),
            Assertion::ObjectPropertyAssertion(val) => GetTarget::get_target(val),
            Assertion::NegativeObjectPropertyAssertion(val) => GetTarget::get_target(val),
        }
    }
}

impl GetTarget for ontology::ClassAssertion {
    fn get_target(&self) -> Option<&[u8]> {
        Some(&self.class)
    }
}

impl GetTarget for ontology::NegativeClassAssertion {
    fn get_target(&self) -> Option<&[u8]> {
        Some(&self.class)
    }
}

impl GetTarget for ontology::DataPropertyAssertion {
    fn get_target(&self) -> Option<&[u8]> {
        self.target.as_ref().map(|n| n.as_slice())
    }
}

impl GetTarget for ontology::NegativeDataPropertyAssertion {
    fn get_target(&self) -> Option<&[u8]> {
        self.target.as_ref().map(|n| n.as_slice())
    }
}

impl GetTarget for ontology::ObjectPropertyAssertion {
    fn get_target(&self) -> Option<&[u8]> {
        self.target.as_ref().map(|n| n.as_slice())
    }
}

impl GetTarget for ontology::NegativeObjectPropertyAssertion {
    fn get_target(&self) -> Option<&[u8]> {
        self.target.as_ref().map(|n| n.as_slice())
    }
}