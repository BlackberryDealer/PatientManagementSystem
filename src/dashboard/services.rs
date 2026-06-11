use crate::dashboard::models::{DashboardStats, DoctorLoad};
use crate::errors::AppError;
use sqlx::SqlitePool;

async fn count(pool: &SqlitePool, sql: &str) -> Result<i64, AppError> {
    let row: (i64,) = sqlx::query_as(sql).fetch_one(pool).await?;
    Ok(row.0)
}

/// Gather the aggregate statistics shown on the admin dashboard.
pub async fn gather_stats(pool: &SqlitePool) -> Result<DashboardStats, AppError> {
    let today = chrono::Utc::now().date_naive();
    let today_str = today.format("%Y-%m-%d").to_string();
    let week_end = (today + chrono::Duration::days(6)).format("%Y-%m-%d").to_string();

    let total_patients = count(pool, "SELECT COUNT(*) FROM patients").await?;
    let total_doctors = count(pool, "SELECT COUNT(*) FROM doctors").await?;

    let appointments_today: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM appointments WHERE appointment_date = ? AND status = 'scheduled'",
    )
    .bind(&today_str)
    .fetch_one(pool)
    .await?;

    let appointments_this_week: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM appointments
         WHERE appointment_date >= ? AND appointment_date <= ? AND status = 'scheduled'",
    )
    .bind(&today_str)
    .bind(&week_end)
    .fetch_one(pool)
    .await?;

    let scheduled_count =
        count(pool, "SELECT COUNT(*) FROM appointments WHERE status = 'scheduled'").await?;
    let completed_count =
        count(pool, "SELECT COUNT(*) FROM appointments WHERE status = 'completed'").await?;
    let cancelled_count =
        count(pool, "SELECT COUNT(*) FROM appointments WHERE status = 'cancelled'").await?;
    let waitlist_waiting =
        count(pool, "SELECT COUNT(*) FROM waitlist WHERE status = 'waiting'").await?;
    let pending_invoices =
        count(pool, "SELECT COUNT(*) FROM invoices WHERE status = 'pending'").await?;

    let billed_total: (Option<f64>,) = sqlx::query_as(
        "SELECT SUM(total_amount) FROM invoices WHERE status != 'cancelled'",
    )
    .fetch_one(pool)
    .await?;

    let collected_total: (Option<f64>,) =
        sqlx::query_as("SELECT SUM(amount) FROM payments").fetch_one(pool).await?;

    let busiest_doctors = sqlx::query_as::<_, (String, i64)>(
        "SELECT u.full_name, COUNT(a.id) AS cnt
         FROM doctors d
         JOIN users u ON d.user_id = u.id
         JOIN appointments a ON a.doctor_id = d.id AND a.status = 'scheduled'
         GROUP BY d.id, u.full_name
         ORDER BY cnt DESC, u.full_name ASC
         LIMIT 5",
    )
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|(doctor_name, appointment_count)| DoctorLoad { doctor_name, appointment_count })
    .collect();

    Ok(DashboardStats {
        total_patients,
        total_doctors,
        appointments_today: appointments_today.0,
        appointments_this_week: appointments_this_week.0,
        scheduled_count,
        completed_count,
        cancelled_count,
        waitlist_waiting,
        billed_total: billed_total.0.unwrap_or(0.0),
        collected_total: collected_total.0.unwrap_or(0.0),
        pending_invoices,
        busiest_doctors,
    })
}
