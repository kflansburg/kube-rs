//! API helpers for structured interaction with the Kubernetes API

use serde::{Deserialize, Serialize};

/// Empty struct for when data should be discarded
///
/// Not using [`()`](https://doc.rust-lang.org/stable/std/primitive.unit.html), because serde's
/// [`Deserialize`](serde::Deserialize) `impl` is too strict.
#[derive(Clone, Deserialize, Serialize, Default, Debug)]
pub struct NotUsed {}

pub(crate) mod typed;
pub use typed::Api;

#[cfg(feature = "ws")] mod remote_command;
#[cfg(feature = "ws")] pub use remote_command::AttachedProcess;

mod subresource;
#[cfg(feature = "ws")]
pub use subresource::{AttachParams, Attachable, Executable};
pub use subresource::{EvictParams, Evictable, LogParams, Loggable, ScaleSpec, ScaleStatus};

// Re-exports from kube-core
pub(crate) use kube_core::params;
pub use kube_core::{
    dynamic::{ApiResource, DynamicObject},
    gvk::{GroupVersionKind, GroupVersionResource},
    metadata::{ListMeta, ObjectMeta, Resource, ResourceExt, TypeMeta},
    object::{Object, ObjectList, WatchEvent},
    request::Request,
};
pub use params::{
    DeleteParams, ListParams, Patch, PatchParams, PostParams, Preconditions, PropagationPolicy,
};

#[cfg(feature = "admission")] pub mod admission;
