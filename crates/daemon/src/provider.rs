use accounts_core::ProviderManifest;
use zbus::{fdo::Result, interface};

/// Exposes a provider manifest loaded at daemon startup.
pub struct ProviderInterface {
    manifest: ProviderManifest,
}

impl ProviderInterface {
    pub fn new(manifest: ProviderManifest) -> Self {
        Self { manifest }
    }
}

#[interface(name = "dev.edfloreshz.Accounts.Provider")]
impl ProviderInterface {
    #[zbus(property)]
    async fn id(&self) -> Result<String> {
        Ok(self.manifest.provider.id.clone())
    }

    #[zbus(property)]
    async fn name(&self) -> Result<String> {
        Ok(self.manifest.provider.name.clone())
    }

    #[zbus(property)]
    async fn icon_name(&self) -> Result<String> {
        Ok(self.manifest.provider.icon.clone().unwrap_or_default())
    }

    #[zbus(property)]
    async fn services(&self) -> Result<Vec<String>> {
        Ok(self.manifest.provider.services.clone())
    }

    #[zbus(property)]
    async fn auth_method(&self) -> Result<String> {
        Ok("oauth2".to_string())
    }
}
