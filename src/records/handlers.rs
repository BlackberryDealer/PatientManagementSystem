use actix_web::{web, HttpResponse};
use tera::Context;

use crate::auth::{require_doctor, AuthUser};
use crate::errors::AppError;
use crate::records::models::CreateRecordForm;
use crate::records::services;

/// GET /records — list records filtered by role
pub async fn list_records(
    pool: web::Data<sqlx::SqlitePool>,
    tera: web::Data<tera::Tera>,
    user: AuthUser,
) -> Result<HttpResponse, AppError> {
    let records = match user.role.as_str() {
        "patient" => {
            services::get_records_for_patient(pool.get_ref(), user.user_id).await?
        }
        "admin" => services::get_all_records(pool.get_ref()).await?,
        _ => {
            require_doctor(&user)?;
            services::get_all_records(pool.get_ref()).await?
        }
    };

    let prescriptions = if user.role == "patient" {
        Some(services::get_prescriptions_for_patient(pool.get_ref(), user.user_id).await?)
    } else {
        None
    };

    let mut ctx = Context::new();
    ctx.insert("user", &user);
    ctx.insert("records", &records);
    ctx.insert("prescriptions", &prescriptions);
    ctx.insert("title", "Medical Records");
    let rendered = tera.render("records/list.html.tera", &ctx)?;
    Ok(HttpResponse::Ok().body(rendered))
}

/// GET /records/create — show create record form (doctor only)
pub async fn create_record_form(
    tera: web::Data<tera::Tera>,
    user: AuthUser,
) -> Result<HttpResponse, AppError> {
    require_doctor(&user)?;

    let mut ctx = Context::new();
    ctx.insert("user", &user);
    ctx.insert("title", "Create Medical Record");
    let rendered = tera.render("records/create.html.tera", &ctx)?;
    Ok(HttpResponse::Ok().body(rendered))
}

/// POST /records/create — create a new medical record (doctor only)
pub async fn create_record(
    pool: web::Data<sqlx::SqlitePool>,
    user: AuthUser,
    form: web::Form<CreateRecordForm>,
) -> Result<HttpResponse, AppError> {
    require_doctor(&user)?;

    let record = services::create_record(pool.get_ref(), user.user_id, &form).await?;

    Ok(HttpResponse::SeeOther()
        .append_header(("Location", format!("/records/{}", record.id)))
        .finish())
}

/// GET /records/{id} — view a single medical record
pub async fn record_detail(
    pool: web::Data<sqlx::SqlitePool>,
    tera: web::Data<tera::Tera>,
    path: web::Path<i64>,
    user: AuthUser,
) -> Result<HttpResponse, AppError> {
    let record_id = path.into_inner();
    let record = services::get_record_by_id(pool.get_ref(), record_id).await?;

    let mut ctx = Context::new();
    ctx.insert("user", &user);
    ctx.insert("record", &record);
    ctx.insert("title", &format!("Medical Record #{}", record.id));
    let rendered = tera.render("records/detail.html.tera", &ctx)?;
    Ok(HttpResponse::Ok().body(rendered))
}
