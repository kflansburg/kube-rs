use crate::{
    api::{
        metadata::{ListMeta, ObjectMeta, TypeMeta},
        GroupVersionKind, Resource,
    },
    error::ErrorResponse,
};
use serde::{Deserialize, Serialize};
use std::{borrow::Cow, fmt::Debug};

/// A raw event returned from a watch query
///
/// Note that a watch query returns many of these as newline separated JSON.
#[derive(Deserialize, Serialize, Clone)]
#[serde(tag = "type", content = "object", rename_all = "UPPERCASE")]
pub enum WatchEvent<K> {
    /// Resource was added
    Added(K),
    /// Resource was modified
    Modified(K),
    /// Resource was deleted
    Deleted(K),
    /// Resource bookmark. `Bookmark` is a slimmed down `K` due to [#285](https://github.com/clux/kube-rs/issues/285).
    ///
    /// From [Watch bookmarks](https://kubernetes.io/docs/reference/using-api/api-concepts/#watch-bookmarks).
    ///
    /// NB: This became Beta first in Kubernetes 1.16.
    Bookmark(Bookmark),
    /// There was some kind of error
    Error(ErrorResponse),
}

impl<K> Debug for WatchEvent<K> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match &self {
            WatchEvent::Added(_) => write!(f, "Added event"),
            WatchEvent::Modified(_) => write!(f, "Modified event"),
            WatchEvent::Deleted(_) => write!(f, "Deleted event"),
            WatchEvent::Bookmark(_) => write!(f, "Bookmark event"),
            WatchEvent::Error(e) => write!(f, "Error event: {:?}", e),
        }
    }
}

/// Slimed down K for [`WatchEvent::Bookmark`] due to [#285](https://github.com/clux/kube-rs/issues/285).
///
/// Can only be relied upon to have metadata with resource version.
/// Bookmarks contain apiVersion + kind + basically empty metadata.
#[derive(Serialize, Deserialize, Clone)]
pub struct Bookmark {
    /// apiVersion + kind
    #[serde(flatten)]
    pub types: TypeMeta,

    /// Basically empty metadata
    pub metadata: BookmarkMeta,
}

/// Slimed down Metadata for WatchEvent::Bookmark
#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BookmarkMeta {
    pub resource_version: String,
}

// -------------------------------------------------------

/// A standard Kubernetes object with `.spec` and `.status`.
///
/// This is a convenience struct provided for serialization/deserialization.
/// It is slightly stricter than ['DynamicObject`] in that it enforces this newer struct
/// spec/status convention, and as such will not work in general with all api-discovered resources.
///
/// This can be used to tie existing resources to smaller, local struct variants to optimize for memory use.
/// E.g. if you are only interested in a few fields, but you store tons of them in memory with reflectors.
#[derive(Deserialize, Serialize, Clone)]
pub struct Object<P, U>
where
    P: Clone,
    U: Clone,
{
    /// The types field of an `Object`
    #[serde(flatten)]
    pub types: TypeMeta,

    /// Resource metadata
    ///
    /// Contains information common to most resources about the Resource,
    /// including the object name, annotations, labels and more.
    pub metadata: ObjectMeta,

    /// The Spec struct of a resource. I.e. `PodSpec`, `DeploymentSpec`, etc.
    ///
    /// This defines the desired state of the Resource as specified by the user.
    pub spec: P,

    /// The Status of a resource. I.e. `PodStatus`, `DeploymentStatus`, etc.
    ///
    /// This publishes the state of the Resource as observed by the controller.
    /// Use `U = NotUsed` when a status does not exist.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<U>,
}

impl<P, U> Object<P, U>
where
    P: Clone,
    U: Clone,
{
    /// A constructor that takes Resource values from a `GroupVersionKind`
    pub fn new(name: &str, gvk: &GroupVersionKind, spec: P) -> Self {
        Self {
            types: TypeMeta {
                api_version: gvk.api_version.to_string(),
                kind: gvk.kind.to_string(),
            },
            metadata: ObjectMeta {
                name: Some(name.to_string()),
                ..Default::default()
            },
            spec: spec,
            status: None,
        }
    }
}

impl<P, U> Resource for Object<P, U>
where
    P: Clone,
    U: Clone,
{
    type DynamicType = GroupVersionKind;

    fn group(dt: &GroupVersionKind) -> Cow<'_, str> {
        dt.group.as_str().into()
    }

    fn version(dt: &GroupVersionKind) -> Cow<'_, str> {
        dt.version.as_str().into()
    }

    fn kind(dt: &GroupVersionKind) -> Cow<'_, str> {
        dt.kind.as_str().into()
    }

    fn api_version(dt: &GroupVersionKind) -> Cow<'_, str> {
        dt.api_version.as_str().into()
    }

    fn plural(dt: &Self::DynamicType) -> Cow<'_, str> {
        if let Some(plural) = &dt.plural {
            plural.into()
        } else {
            // fallback to inference
            crate::api::metadata::to_plural(&Self::kind(dt).to_ascii_lowercase()).into()
        }
    }

    fn meta(&self) -> &ObjectMeta {
        &self.metadata
    }

    fn name(&self) -> String {
        self.metadata.name.clone().expect("missing name")
    }

    fn namespace(&self) -> Option<String> {
        self.metadata.namespace.clone()
    }

    fn resource_ver(&self) -> Option<String> {
        self.metadata.resource_version.clone()
    }
}

/// A generic Kubernetes object list
///
/// This is used instead of a full struct for `DeploymentList`, `PodList`, etc.
/// Kubernetes' API [always seem to expose list structs in this manner](https://docs.rs/k8s-openapi/0.10.0/k8s_openapi/apimachinery/pkg/apis/meta/v1/struct.ObjectMeta.html?search=List).
///
/// Note that this is only used internally within reflectors and informers,
/// and is generally produced from list/watch/delete collection queries on an [`Resource`](super::Resource).
///
/// This is almost equivalent to [`k8s_openapi::List<T>`](k8s_openapi::List), but iterable.
#[derive(Deserialize, Debug)]
pub struct ObjectList<T>
where
    T: Clone,
{
    // NB: kind and apiVersion can be set here, but no need for it atm
    /// ListMeta - only really used for its `resourceVersion`
    ///
    /// See [ListMeta](k8s_openapi::apimachinery::pkg::apis::meta::v1::ListMeta)
    pub metadata: ListMeta,

    /// The items we are actually interested in. In practice; `T := Resource<T,U>`.
    #[serde(bound(deserialize = "Vec<T>: Deserialize<'de>"))]
    pub items: Vec<T>,
}

impl<T: Clone> ObjectList<T> {
    /// `iter` returns an Iterator over the elements of this ObjectList
    ///
    /// # Example
    ///
    /// ```
    /// use kube::api::{ListMeta, ObjectList};
    ///
    /// let metadata: ListMeta = Default::default();
    /// let items = vec![1, 2, 3];
    /// let objectlist = ObjectList { metadata, items };
    ///
    /// let first = objectlist.iter().next();
    /// println!("First element: {:?}", first); // prints "First element: Some(1)"
    /// ```
    pub fn iter<'a>(&'a self) -> impl Iterator<Item = &T> + 'a {
        self.items.iter()
    }

    /// `iter_mut` returns an Iterator of mutable references to the elements of this ObjectList
    ///
    /// # Example
    ///
    /// ```
    /// use kube::api::{ObjectList, ListMeta};
    ///
    /// let metadata: ListMeta = Default::default();
    /// let items = vec![1, 2, 3];
    /// let mut objectlist = ObjectList { metadata, items };
    ///
    /// let mut first = objectlist.iter_mut().next();
    ///
    /// // Reassign the value in first
    /// if let Some(elem) = first {
    ///     *elem = 2;
    ///     println!("First element: {:?}", elem); // prints "First element: 2"
    /// }

    pub fn iter_mut<'a>(&'a mut self) -> impl Iterator<Item = &mut T> + 'a {
        self.items.iter_mut()
    }
}

impl<T: Clone> IntoIterator for ObjectList<T> {
    type IntoIter = ::std::vec::IntoIter<Self::Item>;
    type Item = T;

    fn into_iter(self) -> Self::IntoIter {
        self.items.into_iter()
    }
}

impl<'a, T: Clone> IntoIterator for &'a ObjectList<T> {
    type IntoIter = ::std::slice::Iter<'a, T>;
    type Item = &'a T;

    fn into_iter(self) -> Self::IntoIter {
        self.items.iter()
    }
}

impl<'a, T: Clone> IntoIterator for &'a mut ObjectList<T> {
    type IntoIter = ::std::slice::IterMut<'a, T>;
    type Item = &'a mut T;

    fn into_iter(self) -> Self::IntoIter {
        self.items.iter_mut()
    }
}
