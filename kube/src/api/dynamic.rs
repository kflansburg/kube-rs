use crate::{
    api::{metadata::TypeMeta, Resource},
    Error, Result,
};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::{APIResource, ObjectMeta};
use std::borrow::Cow;

/// Represents a type-erased object kind
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct GroupVersionKind {
    /// API group
    pub group: String,
    /// Version
    pub version: String,
    /// Kind
    pub kind: String,
    /// Concatenation of group and version
    pub api_version: String,
    /// Optional plural/resource
    pub plural: Option<String>,
}

impl GroupVersionKind {
    /// Creates `GroupVersionKind` from an [`APIResource`].
    ///
    /// `APIResource` objects can be extracted from [`Client::list_api_group_resources`].
    /// If it does not specify version and/or group, they will be taken from `group_version`.
    ///
    /// ### Example usage:
    /// ```
    /// use kube::api::{GroupVersionKind, Api, DynamicObject};
    /// # async fn scope(client: kube::Client) -> Result<(), Box<dyn std::error::Error>> {
    /// let apps = client.list_api_group_resources("apps/v1").await?;
    /// for ar in &apps.resources {
    ///     let gvk = GroupVersionKind::from_api_resource(ar, &apps.group_version);
    ///     dbg!(&gvk);
    ///     let api: Api<DynamicObject> = Api::namespaced_with(client.clone(), "default", &gvk);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn from_api_resource(ar: &APIResource, group_version: &str) -> Self {
        let gvsplit = group_version.splitn(2, '/').collect::<Vec<_>>();
        let (default_group, default_version) = match *gvsplit.as_slice() {
            [g, v] => (g, v), // standard case
            [v] => ("", v),   // core v1 case
            _ => unreachable!(),
        };
        let group = ar.group.clone().unwrap_or_else(|| default_group.into());
        let version = ar.version.clone().unwrap_or_else(|| default_version.into());
        let kind = ar.kind.to_string();
        let api_version = if group.is_empty() {
            version.clone()
        } else {
            format!("{}/{}", group, version)
        };
        let plural = Some(ar.name.clone());
        Self {
            group,
            version,
            kind,
            api_version,
            plural,
        }
    }

    /// Set the api group, version, and kind for a resource
    pub fn gvk(group_: &str, version_: &str, kind_: &str) -> Result<Self> {
        let version = version_.to_string();
        let group = group_.to_string();
        let kind = kind_.to_string();
        let api_version = if group.is_empty() {
            version.to_string()
        } else {
            format!("{}/{}", group, version)
        };
        if version.is_empty() {
            return Err(Error::DynamicType(format!(
                "GroupVersionKind '{}' must have a version",
                kind
            )));
        }
        if kind.is_empty() {
            return Err(Error::DynamicType(format!(
                "GroupVersionKind '{}' must have a kind",
                kind
            )));
        }
        Ok(Self {
            group,
            version,
            kind,
            api_version,
            plural: None,
        })
    }

    /// Set an explicit plural/resource value to avoid relying on inferred pluralisation.
    pub fn plural(mut self, plural: &str) -> Self {
        self.plural = Some(plural.to_string());
        self
    }
}

/// A dynamic representation of a kubernetes resource
///
/// This will work with any non-list type object.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct DynamicObject {
    /// The type fields, not always present
    #[serde(flatten, default)]
    pub types: Option<TypeMeta>,
    /// Object metadata
    pub metadata: ObjectMeta,

    /// All other keys
    #[serde(flatten)]
    pub data: serde_json::Value,
}

impl DynamicObject {
    /// Create a DynamicObject with minimal values set from GVK.
    pub fn new(name: &str, gvk: &GroupVersionKind) -> Self {
        Self {
            types: Some(TypeMeta {
                api_version: gvk.api_version.to_string(),
                kind: gvk.kind.to_string(),
            }),
            metadata: ObjectMeta {
                name: Some(name.to_string()),
                ..Default::default()
            },
            data: Default::default(),
        }
    }

    /// Attach dynamic data to a DynamicObject
    pub fn data(mut self, data: serde_json::Value) -> Self {
        self.data = data;
        self
    }

    /// Attach a namespace to a DynamicObject
    pub fn namespace(mut self, ns: &str) -> Self {
        self.metadata.namespace = Some(ns.into());
        self
    }
}

impl Resource for DynamicObject {
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

#[cfg(test)]
mod test {
    use crate::{
        api::{DynamicObject, GroupVersionKind, Patch, PatchParams, PostParams, Request, Resource},
        Result,
    };
    #[test]
    fn raw_custom_resource() {
        let gvk = GroupVersionKind::gvk("clux.dev", "v1", "Foo").unwrap();
        let url = DynamicObject::url_path(&gvk, Some("myns"));

        let pp = PostParams::default();
        let req = Request::new(&url).create(&pp, vec![]).unwrap();
        assert_eq!(req.uri(), "/apis/clux.dev/v1/namespaces/myns/foos?");
        let patch_params = PatchParams::default();
        let req = Request::new(url)
            .patch("baz", &patch_params, &Patch::Merge(()))
            .unwrap();
        assert_eq!(req.uri(), "/apis/clux.dev/v1/namespaces/myns/foos/baz?");
        assert_eq!(req.method(), "PATCH");
    }

    #[test]
    fn raw_resource_in_default_group() -> Result<()> {
        let gvk = GroupVersionKind::gvk("", "v1", "Service").unwrap();
        let url = DynamicObject::url_path(&gvk, None);
        let pp = PostParams::default();
        let req = Request::new(url).create(&pp, vec![])?;
        assert_eq!(req.uri(), "/api/v1/services?");
        Ok(())
    }

    #[cfg(feature = "derive")]
    #[tokio::test]
    #[ignore] // circle has no kubeconfig
    async fn convenient_custom_resource() {
        use crate as kube; // derive macro needs kube in scope
        use crate::{Api, Client, CustomResource};
        use schemars::JsonSchema;
        use serde::{Deserialize, Serialize};
        #[derive(Clone, Debug, CustomResource, Deserialize, Serialize, JsonSchema)]
        #[kube(group = "clux.dev", version = "v1", kind = "Foo", namespaced)]
        struct FooSpec {
            foo: String,
        }
        let client = Client::try_default().await.unwrap();

        let gvk = GroupVersionKind::gvk("clux.dev", "v1", "Foo").unwrap();
        let a1: Api<DynamicObject> = Api::namespaced_with(client.clone(), "myns", &gvk);
        let a2: Api<Foo> = Api::namespaced(client.clone(), "myns");

        // make sure they return the same url_path through their impls
        assert_eq!(a1.request.url_path, a2.request.url_path);
    }
}
