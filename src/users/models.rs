use serde::{Deserialize, Serialize};

// ============================================================
// User — core authentication entity
// ============================================================

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct User {
    pub id: i64,
    pub username: String,
    pub email: String,
    #[serde(skip_serializing)]
    password_hash: String,  // private — accessed only through verify_password()
    role: String,           // private — accessed only through role() / role_str()
    pub full_name: String,
    pub created_at: chrono::NaiveDateTime,
}

impl User {
    /// The user's role as the typed `Role` enum rather than the raw stored
    /// string, so callers get an exhaustive match instead of stringly-typed
    /// comparisons. An unrecognised value falls back to the least-privileged
    /// role (see `Role::from_session`).
    pub fn role(&self) -> crate::auth::Role {
        crate::auth::Role::from_session(&self.role)
    }

    /// The raw role string — only for code that must write a `&str` directly
    /// (session storage, audit log). Prefer `role()` for all comparisons.
    pub fn role_str(&self) -> &str {
        &self.role
    }

    /// Verify a plaintext password against the stored bcrypt hash.
    /// Encapsulates the credential comparison so no caller ever needs to
    /// read `password_hash` directly; the field stays private.
    pub fn verify_password(&self, candidate: &str) -> Result<bool, bcrypt::BcryptError> {
        bcrypt::verify(candidate, &self.password_hash)
    }
}

#[derive(Debug, Deserialize)]
pub struct RegisterForm {
    pub username: String,
    pub email: String,
    pub password: String,
    pub full_name: String,
    pub role: String, // "patient", "doctor", or "admin"
}

impl RegisterForm {
    /// All registration input rules in one place, checked before any
    /// hashing or database work happens.
    pub fn validate(&self) -> Result<(), crate::errors::AppError> {
        use crate::errors::AppError;
        if self.username.trim().len() < 3 {
            return Err(AppError::BadRequest(
                "Username must be at least 3 characters".into(),
            ));
        }
        if !self.email.contains('@') {
            return Err(AppError::BadRequest(
                "A valid email address is required".into(),
            ));
        }
        if self.password.len() < 8 {
            return Err(AppError::BadRequest(
                "Password must be at least 8 characters".into(),
            ));
        }
        if self.full_name.trim().is_empty() {
            return Err(AppError::BadRequest("Full name is required".into()));
        }
        // Public self-registration is restricted to patients. Staff accounts
        // (doctor/admin) are provisioned by an administrator or the seed script
        // — never chosen by the registrant. Otherwise anyone could submit
        // `role=admin` and grant themselves full access to the whole system.
        if self.role != "patient" {
            return Err(AppError::BadRequest(
                "Public registration is for patients only. Staff accounts are created by an administrator."
                    .into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Deserialize)]
pub struct LoginForm {
    pub login: String,    // username or email
    pub password: String,
}

impl LoginForm {
    /// Reject obviously empty credentials before the database lookup, so the
    /// login path follows the same Route -> Validation -> DB shape as every
    /// other form. The message is intentionally identical to an
    /// authentication failure so an empty field leaks nothing about which
    /// half was missing.
    pub fn validate(&self) -> Result<(), crate::errors::AppError> {
        if self.login.trim().is_empty() || self.password.is_empty() {
            return Err(crate::errors::AppError::Unauthorized(
                "Invalid username/email or password".into(),
            ));
        }
        Ok(())
    }
}

/// Form an administrator uses to create a staff (doctor/admin) account — the
/// privileged counterpart to `RegisterForm`, which is restricted to patients.
#[derive(Debug, Deserialize)]
pub struct CreateStaffForm {
    pub username: String,
    pub email: String,
    pub password: String,
    pub full_name: String,
    pub role: String, // "doctor" or "admin"
    // Doctor-only profile fields (ignored for admin).
    pub specialization: Option<String>,
    pub license_number: Option<String>,
}

impl CreateStaffForm {
    pub fn validate(&self) -> Result<(), crate::errors::AppError> {
        use crate::errors::AppError;
        if self.username.trim().len() < 3 {
            return Err(AppError::BadRequest(
                "Username must be at least 3 characters".into(),
            ));
        }
        if !self.email.contains('@') {
            return Err(AppError::BadRequest(
                "A valid email address is required".into(),
            ));
        }
        if self.password.len() < 8 {
            return Err(AppError::BadRequest(
                "Password must be at least 8 characters".into(),
            ));
        }
        if self.full_name.trim().is_empty() {
            return Err(AppError::BadRequest("Full name is required".into()));
        }
        // Only staff roles may be created here — patients self-register.
        if !["doctor", "admin"].contains(&self.role.as_str()) {
            return Err(AppError::BadRequest(
                "Role must be either doctor or admin".into(),
            ));
        }
        Ok(())
    }
}

// ============================================================
// Edit profile form
// ============================================================

#[derive(Debug, Deserialize)]
pub struct EditProfileForm {
    pub full_name: String,
    pub email: String,
    // Patient fields
    pub date_of_birth: Option<String>,
    pub phone: Option<String>,
    pub address: Option<String>,
    pub blood_group: Option<String>,
    pub emergency_contact: Option<String>,
    // Doctor fields
    pub specialization: Option<String>,
    pub license_number: Option<String>,
}

impl EditProfileForm {
    /// Profile edits must satisfy the same core rules as registration:
    /// a non-empty full name and a plausible email address. Checked
    /// before anything touches the database, mirroring `RegisterForm`.
    pub fn validate(&self) -> Result<(), crate::errors::AppError> {
        use crate::errors::AppError;
        if self.full_name.trim().is_empty() {
            return Err(AppError::BadRequest("Full name is required".into()));
        }
        if !self.email.contains('@') {
            return Err(AppError::BadRequest(
                "A valid email address is required".into(),
            ));
        }
        if let Some(bg) = &self.blood_group {
            if !bg.is_empty() && !Patient::is_valid_blood_group(bg) {
                return Err(AppError::BadRequest(
                    "Blood group must be one of: A+, A-, B+, B-, AB+, AB-, O+, O-".into(),
                ));
            }
        }
        Ok(())
    }
}

// ============================================================
// Patient — extended profile for patient-role users
// ============================================================

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct Patient {
    pub id: i64,
    pub user_id: i64,
    pub date_of_birth: Option<chrono::NaiveDate>,
    pub phone: Option<String>,
    pub address: Option<String>,
    pub blood_group: Option<String>,
    pub emergency_contact: Option<String>,
}

impl Patient {
    /// The eight ABO+Rh blood groups, in display order. Single source of
    /// truth shared by `is_valid_blood_group` (validation) and the
    /// edit-profile dropdown (presentation), mirroring how
    /// `appointments::services::start_time_slots` feeds the booking form —
    /// the canonical list is never duplicated in a template.
    pub const BLOOD_GROUPS: [&'static str; 8] =
        ["A+", "A-", "B+", "B-", "AB+", "AB-", "O+", "O-"];

    /// Whether `value` is a recognised blood group. Trimmed and upper-cased
    /// before matching so legacy/free-form values like `"o+"` or `" O+ "`
    /// still validate against the canonical set.
    pub fn is_valid_blood_group(value: &str) -> bool {
        Self::BLOOD_GROUPS.contains(&value.trim().to_uppercase().as_str())
    }
}

// ============================================================
// Doctor — extended profile for doctor-role users
// ============================================================

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct Doctor {
    pub id: i64,
    pub user_id: i64,
    pub specialization: String,
    pub license_number: String,
    pub phone: Option<String>,
}

impl Doctor {
    /// Default specialization for a newly-provisioned doctor whose profile
    /// hasn't been filled in yet. Single source of truth for the value that
    /// was previously hard-coded across the registration/staff-creation/
    /// profile-update paths.
    pub const DEFAULT_SPECIALIZATION: &'static str = "General Practice";

    /// Default licence-number placeholder until an admin records the real one.
    pub const DEFAULT_LICENSE: &'static str = "PENDING";
}
