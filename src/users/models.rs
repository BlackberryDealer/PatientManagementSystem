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
    pub password_hash: String,
    pub role: String,
    pub full_name: String,
    pub created_at: chrono::NaiveDateTime,
}

#[derive(Debug, Deserialize)]
pub struct RegisterForm {
    pub username: String,
    pub email: String,
    pub password: String,
    pub full_name: String,
    pub role: String, // "patient", "doctor", or "admin"
}

#[derive(Debug, Deserialize)]
pub struct LoginForm {
    pub username: String,
    pub password: String,
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
