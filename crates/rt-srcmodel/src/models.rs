//! The set of Source_Models a caller has loaded.

use crate::SourceModel;
use rt_foundation::DocumentId;
use std::collections::BTreeMap;
use std::fmt;

/// Two models were given for one document identifier.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DuplicateDocument(pub DocumentId);

impl fmt::Display for DuplicateDocument {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "more than one Source_Model for document '{}'", self.0)
    }
}

impl std::error::Error for DuplicateDocument {}

/// Loaded Source_Models by document identifier, iterated in ascending identifier order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SourceModels {
    models: BTreeMap<DocumentId, SourceModel>,
}

impl SourceModels {
    /// An empty set.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a model.
    ///
    /// # Errors
    /// [`DuplicateDocument`] when a model for the same document is already present. The set is
    /// unchanged.
    pub fn insert(&mut self, model: SourceModel) -> Result<(), DuplicateDocument> {
        let id = model.document().clone();
        if self.models.contains_key(&id) {
            return Err(DuplicateDocument(id));
        }
        self.models.insert(id, model);
        Ok(())
    }

    /// The model of a document.
    #[must_use]
    pub fn get(&self, document: &DocumentId) -> Option<&SourceModel> {
        self.models.get(document)
    }

    /// The models in ascending document identifier order.
    pub fn iter(&self) -> impl Iterator<Item = &SourceModel> {
        self.models.values()
    }

    /// Number of models.
    #[must_use]
    pub fn len(&self) -> usize {
        self.models.len()
    }

    /// True when no model is present.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.models.is_empty()
    }
}
