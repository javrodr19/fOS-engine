//! Form Autofill
//!
//! Profile-based autofill for addresses, names, and credit cards.

use std::collections::HashMap;

/// Autofill field type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AutofillFieldType {
    // Name fields
    Name,
    GivenName,
    AdditionalName,
    FamilyName,
    HonorificPrefix,
    HonorificSuffix,
    Nickname,
    
    // Contact fields
    Email,
    Tel,
    TelCountryCode,
    TelNational,
    TelAreaCode,
    TelLocal,
    
    // Address fields
    StreetAddress,
    AddressLine1,
    AddressLine2,
    AddressLine3,
    AddressLevel1, // State/Province
    AddressLevel2, // City
    AddressLevel3, // District
    AddressLevel4, // Neighborhood
    Country,
    CountryName,
    PostalCode,
    
    // Credit card fields
    CcName,
    CcGivenName,
    CcFamilyName,
    CcNumber,
    CcExp,
    CcExpMonth,
    CcExpYear,
    CcCsc,
    CcType,
    
    // Other
    Organization,
    OrganizationTitle,
    Username,
    NewPassword,
    CurrentPassword,
    OneTimeCode,
    Bday,
    BdayDay,
    BdayMonth,
    BdayYear,
    Sex,
    Url,
    Photo,
}

impl AutofillFieldType {
    /// Parse from autocomplete attribute value
    pub fn parse(value: &str) -> Option<Self> {
        let value = value.trim().to_lowercase();
        Some(match value.as_str() {
            "name" => Self::Name,
            "given-name" => Self::GivenName,
            "additional-name" => Self::AdditionalName,
            "family-name" => Self::FamilyName,
            "honorific-prefix" => Self::HonorificPrefix,
            "honorific-suffix" => Self::HonorificSuffix,
            "nickname" => Self::Nickname,
            "email" => Self::Email,
            "tel" => Self::Tel,
            "tel-country-code" => Self::TelCountryCode,
            "tel-national" => Self::TelNational,
            "tel-area-code" => Self::TelAreaCode,
            "tel-local" => Self::TelLocal,
            "street-address" => Self::StreetAddress,
            "address-line1" => Self::AddressLine1,
            "address-line2" => Self::AddressLine2,
            "address-line3" => Self::AddressLine3,
            "address-level1" => Self::AddressLevel1,
            "address-level2" => Self::AddressLevel2,
            "address-level3" => Self::AddressLevel3,
            "address-level4" => Self::AddressLevel4,
            "country" => Self::Country,
            "country-name" => Self::CountryName,
            "postal-code" => Self::PostalCode,
            "cc-name" => Self::CcName,
            "cc-given-name" => Self::CcGivenName,
            "cc-family-name" => Self::CcFamilyName,
            "cc-number" => Self::CcNumber,
            "cc-exp" => Self::CcExp,
            "cc-exp-month" => Self::CcExpMonth,
            "cc-exp-year" => Self::CcExpYear,
            "cc-csc" => Self::CcCsc,
            "cc-type" => Self::CcType,
            "organization" => Self::Organization,
            "organization-title" => Self::OrganizationTitle,
            "username" => Self::Username,
            "new-password" => Self::NewPassword,
            "current-password" => Self::CurrentPassword,
            "one-time-code" => Self::OneTimeCode,
            "bday" => Self::Bday,
            "bday-day" => Self::BdayDay,
            "bday-month" => Self::BdayMonth,
            "bday-year" => Self::BdayYear,
            "sex" => Self::Sex,
            "url" => Self::Url,
            "photo" => Self::Photo,
            _ => return None,
        })
    }
    
    /// Check if field contains sensitive data
    pub fn is_sensitive(&self) -> bool {
        matches!(self,
            Self::CcNumber | Self::CcCsc | Self::CcExp | Self::CcExpMonth | Self::CcExpYear |
            Self::CurrentPassword | Self::NewPassword | Self::OneTimeCode
        )
    }
}

/// Autofill profile
#[derive(Debug, Clone, Default)]
pub struct AutofillProfile {
    pub id: u64,
    pub name: String,
    pub fields: HashMap<AutofillFieldType, String>,
    pub created: u64,
    pub last_used: u64,
    pub use_count: u32,
}

impl AutofillProfile {
    pub fn new(id: u64, name: &str) -> Self {
        Self {
            id,
            name: name.to_string(),
            fields: HashMap::new(),
            created: current_time_ms(),
            last_used: 0,
            use_count: 0,
        }
    }
    
    /// Set a field value
    pub fn set(&mut self, field: AutofillFieldType, value: &str) {
        self.fields.insert(field, value.to_string());
    }
    
    /// Get a field value
    pub fn get(&self, field: AutofillFieldType) -> Option<&str> {
        self.fields.get(&field).map(|s| s.as_str())
    }
    
    /// Record usage
    pub fn mark_used(&mut self) {
        self.last_used = current_time_ms();
        self.use_count += 1;
    }
}

/// Credit card profile (stored separately with encryption)
#[derive(Debug, Clone)]
pub struct CreditCardProfile {
    pub id: u64,
    pub cardholder_name: String,
    pub card_number_masked: String, // Only last 4 digits visible
    pub card_number_encrypted: Vec<u8>,
    pub exp_month: u8,
    pub exp_year: u16,
    pub card_type: CardType,
    pub created: u64,
}

/// Card type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardType {
    Visa,
    Mastercard,
    Amex,
    Discover,
    DinersClub,
    Jcb,
    UnionPay,
    Unknown,
}

impl CardType {
    /// Detect card type from number
    pub fn from_number(number: &str) -> Self {
        let digits: String = number.chars().filter(|c| c.is_ascii_digit()).collect();
        if digits.is_empty() {
            return Self::Unknown;
        }
        
        match &digits[..1.min(digits.len())] {
            "4" => Self::Visa,
            "5" => Self::Mastercard,
            "3" if digits.len() > 1 && (digits.starts_with("34") || digits.starts_with("37")) => Self::Amex,
            "6" => Self::Discover,
            _ => Self::Unknown,
        }
    }
}

/// Form field info for autofill detection
#[derive(Debug, Clone)]
pub struct FormField {
    pub element_id: u64,
    pub name: Option<String>,
    pub id: Option<String>,
    pub autocomplete: Option<String>,
    pub field_type: Option<String>,
    pub placeholder: Option<String>,
    pub label: Option<String>,
}

/// Autofill manager
#[derive(Debug, Default)]
pub struct AutofillManager {
    profiles: Vec<AutofillProfile>,
    credit_cards: Vec<CreditCardProfile>,
    next_profile_id: u64,
    enabled: bool,
    save_passwords: bool,
}

impl AutofillManager {
    pub fn new() -> Self {
        Self {
            profiles: Vec::new(),
            credit_cards: Vec::new(),
            next_profile_id: 1,
            enabled: true,
            save_passwords: true,
        }
    }
    
    /// Enable/disable autofill
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }
    
    /// Add a profile
    pub fn add_profile(&mut self, profile: AutofillProfile) -> u64 {
        let id = self.next_profile_id;
        self.next_profile_id += 1;
        self.profiles.push(AutofillProfile { id, ..profile });
        id
    }
    
    /// Get all profiles
    pub fn get_profiles(&self) -> &[AutofillProfile] {
        &self.profiles
    }
    
    /// Delete a profile
    pub fn delete_profile(&mut self, id: u64) {
        self.profiles.retain(|p| p.id != id);
    }
    
    /// Detect field type from attributes
    pub fn detect_field_type(&self, field: &FormField) -> Option<AutofillFieldType> {
        // First check explicit autocomplete attribute
        if let Some(ref autocomplete) = field.autocomplete {
            if autocomplete != "off" && autocomplete != "on" {
                if let Some(field_type) = AutofillFieldType::parse(autocomplete) {
                    return Some(field_type);
                }
            }
        }
        
        // Heuristic detection based on name/id/label
        let indicators = [
            field.name.as_deref(),
            field.id.as_deref(),
            field.placeholder.as_deref(),
            field.label.as_deref(),
        ];
        
        for indicator in indicators.iter().filter_map(|i| *i) {
            let lower = indicator.to_lowercase();
            
            if lower.contains("email") || lower.contains("e-mail") {
                return Some(AutofillFieldType::Email);
            }
            if lower.contains("phone") || lower.contains("tel") || lower.contains("mobile") {
                return Some(AutofillFieldType::Tel);
            }
            if lower.contains("first") && lower.contains("name") {
                return Some(AutofillFieldType::GivenName);
            }
            if lower.contains("last") && lower.contains("name") {
                return Some(AutofillFieldType::FamilyName);
            }
            if lower.contains("name") && !lower.contains("user") {
                return Some(AutofillFieldType::Name);
            }
            if lower.contains("address") || lower.contains("street") {
                return Some(AutofillFieldType::StreetAddress);
            }
            if lower.contains("city") {
                return Some(AutofillFieldType::AddressLevel2);
            }
            if lower.contains("state") || lower.contains("province") {
                return Some(AutofillFieldType::AddressLevel1);
            }
            if lower.contains("zip") || lower.contains("postal") {
                return Some(AutofillFieldType::PostalCode);
            }
            if lower.contains("country") {
                return Some(AutofillFieldType::Country);
            }
            if lower.contains("card") && lower.contains("number") {
                return Some(AutofillFieldType::CcNumber);
            }
            if lower.contains("cvv") || lower.contains("cvc") || lower.contains("csc") {
                return Some(AutofillFieldType::CcCsc);
            }
            if lower.contains("expir") {
                if lower.contains("month") {
                    return Some(AutofillFieldType::CcExpMonth);
                } else if lower.contains("year") {
                    return Some(AutofillFieldType::CcExpYear);
                }
                return Some(AutofillFieldType::CcExp);
            }
            if lower.contains("username") || lower.contains("user") && lower.contains("name") {
                return Some(AutofillFieldType::Username);
            }
            if lower.contains("password") || lower.contains("passwd") {
                if lower.contains("new") || lower.contains("confirm") {
                    return Some(AutofillFieldType::NewPassword);
                }
                return Some(AutofillFieldType::CurrentPassword);
            }
        }
        
        None
    }
    
    /// Get suggestions for a field
    pub fn get_suggestions(&self, field_type: AutofillFieldType) -> Vec<(&AutofillProfile, &str)> {
        if !self.enabled {
            return Vec::new();
        }
        
        self.profiles.iter()
            .filter_map(|profile| {
                profile.get(field_type).map(|value| (profile, value))
            })
            .collect()
    }
    
    /// Fill form fields with profile data
    pub fn fill_form(&mut self, profile_id: u64, fields: &[FormField]) -> HashMap<u64, String> {
        let mut filled = HashMap::new();
        
        // First detect all field types
        let field_types: Vec<_> = fields.iter()
            .map(|f| (f.element_id, self.detect_field_type(f)))
            .collect();
        
        if let Some(profile) = self.profiles.iter_mut().find(|p| p.id == profile_id) {
            profile.mark_used();
            
            for (element_id, field_type) in field_types {
                if let Some(ft) = field_type {
                    if let Some(value) = profile.get(ft) {
                        filled.insert(element_id, value.to_string());
                    }
                }
            }
        }
        
        filled
    }
}

fn current_time_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_autofill_field_type_parse() {
        assert_eq!(AutofillFieldType::parse("email"), Some(AutofillFieldType::Email));
        assert_eq!(AutofillFieldType::parse("cc-number"), Some(AutofillFieldType::CcNumber));
        assert_eq!(AutofillFieldType::parse("invalid"), None);
    }
    
    #[test]
    fn test_autofill_profile() {
        let mut profile = AutofillProfile::new(1, "Home");
        profile.set(AutofillFieldType::Email, "test@example.com");
        
        assert_eq!(profile.get(AutofillFieldType::Email), Some("test@example.com"));
    }
    
    #[test]
    fn test_card_type_detection() {
        assert_eq!(CardType::from_number("4111111111111111"), CardType::Visa);
        assert_eq!(CardType::from_number("5500000000000004"), CardType::Mastercard);
        assert_eq!(CardType::from_number("340000000000009"), CardType::Amex);
    }
    
    #[test]
    fn test_field_detection() {
        let manager = AutofillManager::new();
        
        let field = FormField {
            element_id: 1,
            name: Some("email_address".to_string()),
            id: None,
            autocomplete: None,
            field_type: None,
            placeholder: None,
            label: None,
        };
        
        assert_eq!(manager.detect_field_type(&field), Some(AutofillFieldType::Email));
    }
}
