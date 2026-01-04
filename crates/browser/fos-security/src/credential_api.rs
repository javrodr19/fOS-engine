//! Credential Management API
//!
//! Password, federated, and public key credentials.

/// Credential type
#[derive(Debug, Clone)]
pub enum Credential {
    Password(PasswordCredential),
    Federated(FederatedCredential),
    PublicKey(PublicKeyCredential),
}

impl Credential {
    pub fn id(&self) -> &str {
        match self {
            Self::Password(c) => &c.id, Self::Federated(c) => &c.id, Self::PublicKey(c) => &c.id,
        }
    }
    
    pub fn credential_type(&self) -> &'static str {
        match self { Self::Password(_) => "password", Self::Federated(_) => "federated", Self::PublicKey(_) => "public-key" }
    }
}

/// Password credential
#[derive(Debug, Clone)]
pub struct PasswordCredential {
    pub id: String,
    pub name: String,
    pub icon_url: Option<String>,
    pub password: String,
}

impl PasswordCredential {
    pub fn new(id: &str, password: &str) -> Self {
        Self { id: id.into(), name: String::new(), icon_url: None, password: password.into() }
    }
}

/// Federated credential
#[derive(Debug, Clone)]
pub struct FederatedCredential {
    pub id: String,
    pub name: String,
    pub icon_url: Option<String>,
    pub provider: String,
    pub protocol: Option<String>,
}

impl FederatedCredential {
    pub fn new(id: &str, provider: &str) -> Self {
        Self { id: id.into(), name: String::new(), icon_url: None, provider: provider.into(), protocol: None }
    }
}

/// Public key credential (WebAuthn)
#[derive(Debug, Clone)]
pub struct PublicKeyCredential {
    pub id: String,
    pub raw_id: Vec<u8>,
    pub authenticator_attachment: Option<AuthenticatorAttachment>,
    pub response: AuthenticatorResponse,
}

/// Authenticator attachment
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthenticatorAttachment { Platform, CrossPlatform }

/// Authenticator response
#[derive(Debug, Clone)]
pub enum AuthenticatorResponse {
    Attestation { client_data_json: Vec<u8>, attestation_object: Vec<u8> },
    Assertion { client_data_json: Vec<u8>, authenticator_data: Vec<u8>, signature: Vec<u8>, user_handle: Option<Vec<u8>> },
}

/// Credential request options
#[derive(Debug, Clone, Default)]
pub struct CredentialRequestOptions {
    pub mediation: CredentialMediationRequirement,
    pub password: bool,
    pub federated: Option<FederatedCredentialRequestOptions>,
    pub public_key: Option<PublicKeyCredentialRequestOptions>,
}

/// Mediation requirement
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CredentialMediationRequirement { #[default] Optional, Required, Silent, Conditional }

/// Federated credential options
#[derive(Debug, Clone, Default)]
pub struct FederatedCredentialRequestOptions {
    pub providers: Vec<String>,
    pub protocols: Vec<String>,
}

/// Public key credential options
#[derive(Debug, Clone, Default)]
pub struct PublicKeyCredentialRequestOptions {
    pub challenge: Vec<u8>,
    pub timeout: Option<u32>,
    pub rp_id: Option<String>,
    pub allow_credentials: Vec<PublicKeyCredentialDescriptor>,
    pub user_verification: UserVerificationRequirement,
}

/// Credential descriptor
#[derive(Debug, Clone)]
pub struct PublicKeyCredentialDescriptor {
    pub id: Vec<u8>,
    pub transports: Vec<AuthenticatorTransport>,
}

/// Transport type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthenticatorTransport { Usb, Nfc, Ble, Internal, Hybrid }

/// User verification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum UserVerificationRequirement { #[default] Preferred, Required, Discouraged }

/// Credential manager
#[derive(Debug, Default)]
pub struct CredentialManager {
    stored: std::collections::HashMap<String, Credential>,
}

impl CredentialManager {
    pub fn new() -> Self { Self::default() }
    
    pub fn store(&mut self, credential: Credential) {
        self.stored.insert(credential.id().to_string(), credential);
    }
    
    pub fn get(&self, options: &CredentialRequestOptions) -> Option<&Credential> {
        if options.mediation == CredentialMediationRequirement::Silent { return None; }
        // Simple implementation - return first matching credential
        if options.password {
            return self.stored.values().find(|c| matches!(c, Credential::Password(_)));
        }
        if options.federated.is_some() {
            return self.stored.values().find(|c| matches!(c, Credential::Federated(_)));
        }
        None
    }
    
    pub fn create_password(&mut self, id: &str, password: &str) -> Credential {
        let cred = Credential::Password(PasswordCredential::new(id, password));
        self.store(cred.clone());
        cred
    }
    
    pub fn prevent_silent_access(&mut self) {
        // Mark credentials as requiring mediation
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_password_credential() {
        let cred = PasswordCredential::new("user@example.com", "secret");
        assert_eq!(cred.id, "user@example.com");
    }
    
    #[test]
    fn test_credential_manager() {
        let mut manager = CredentialManager::new();
        manager.create_password("test", "pass");
        
        let options = CredentialRequestOptions { password: true, ..Default::default() };
        assert!(manager.get(&options).is_some());
    }
}
