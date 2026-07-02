//! PDF generation for printable medical reports (advanced feature,
//! project spec: "medical report generation").
//!
//! Renders a [`RecordReportData`] bundle into a self-contained A4 PDF using the
//! pure-Rust `printpdf` crate and the standard built-in Helvetica fonts. There
//! are no external binaries (no wkhtmltopdf/Chrome) and no font files to ship,
//! so `cargo run` still works out of the box. The result is a `Vec<u8>` that the
//! handler streams back with `Content-Type: application/pdf`.
//!
//! Layout is built from a single primitive — `use_text` — tracked by a cursor
//! that walks down the page and starts a new page when it runs out of room, so
//! long records (lots of notes or prescriptions) paginate cleanly. Body text is
//! word-wrapped to the page width by [`wrap`].

use printpdf::{
    BuiltinFont, IndirectFontRef, Mm, PdfDocument, PdfDocumentReference, PdfLayerReference,
};

use crate::errors::AppError;
use crate::records::models::RecordReportData;

// --- Page geometry (A4 portrait), all in millimetres ---
const PAGE_W: f32 = 210.0;
const PAGE_H: f32 = 297.0;
const MARGIN_L: f32 = 20.0;
const MARGIN_TOP: f32 = 277.0; // first baseline, measured from the bottom edge
const MARGIN_BOTTOM: f32 = 18.0; // stop filling the page below this line
const CONTENT_W: f32 = PAGE_W - 2.0 * MARGIN_L; // 170mm usable width

// --- Type sizes (points) ---
const SIZE_TITLE: f32 = 22.0;
const SIZE_SUBTITLE: f32 = 10.0;
const SIZE_LABEL: f32 = 8.5; // section headings ("PATIENT", "DIAGNOSIS"…)
const SIZE_VALUE: f32 = 12.0; // emphasised values (names)
const SIZE_SMALL: f32 = 8.5; // secondary detail lines
const SIZE_BODY: f32 = 10.5; // wrapped paragraph text

// 1 typographic point = 0.352778 mm. Helvetica's average glyph advance is
// roughly half its point size, which is accurate enough to drive word-wrap and
// horizontal centring without pulling in the full font-metric tables.
const PT_TO_MM: f32 = 0.352778;

fn avg_char_mm(size: f32) -> f32 {
    size * 0.5 * PT_TO_MM
}

fn line_h(size: f32) -> f32 {
    size * 1.3 * PT_TO_MM
}

fn text_width_mm(s: &str, size: f32) -> f32 {
    s.chars().count() as f32 * avg_char_mm(size)
}

/// Greedily word-wrap `text` so each line fits within `width_mm` at the given
/// font `size`. Honours existing newlines (each becomes a paragraph break), and
/// hard-breaks any single word longer than the line budget. Operates on `char`
/// boundaries so multi-byte UTF-8 (accented names, etc.) never splits mid-glyph.
fn wrap(text: &str, size: f32, width_mm: f32) -> Vec<String> {
    let max_chars = ((width_mm / avg_char_mm(size)).floor() as usize).max(8);
    let mut out = Vec::new();

    for raw in text.split('\n') {
        let mut cur = String::new();
        let mut cur_len = 0usize;

        for word in raw.split_whitespace() {
            let wlen = word.chars().count();

            // A word too long for any line: flush what we have, then slice the
            // word into max-width chunks on char boundaries.
            if wlen > max_chars {
                if cur_len > 0 {
                    out.push(std::mem::take(&mut cur));
                    cur_len = 0;
                }
                let chars: Vec<char> = word.chars().collect();
                for chunk in chars.chunks(max_chars) {
                    out.push(chunk.iter().collect());
                }
                // Keep building on the trailing chunk so later words can append.
                if let Some(last) = out.pop() {
                    cur_len = last.chars().count();
                    cur = last;
                }
                continue;
            }

            let extra = if cur_len == 0 { wlen } else { wlen + 1 };
            if cur_len + extra > max_chars {
                out.push(std::mem::take(&mut cur));
                cur = word.to_string();
                cur_len = wlen;
            } else {
                if cur_len > 0 {
                    cur.push(' ');
                    cur_len += 1;
                }
                cur.push_str(word);
                cur_len += wlen;
            }
        }
        out.push(cur); // preserve the (possibly empty) final line of the paragraph
    }
    out
}

/// A medical-report PDF under construction. Owns the document, the active page
/// layer, both font weights, and a vertical cursor (`y`) that descends the page.
struct ReportPdf {
    doc: PdfDocumentReference,
    layer: PdfLayerReference,
    regular: IndirectFontRef,
    bold: IndirectFontRef,
    y: f32,
}

impl ReportPdf {
    fn new(title: &str) -> Result<Self, AppError> {
        let (doc, page, layer) =
            PdfDocument::new(title.to_string(), Mm(PAGE_W), Mm(PAGE_H), "Layer 1".to_string());
        let regular = doc
            .add_builtin_font(BuiltinFont::Helvetica)
            .map_err(|e| AppError::Internal(format!("PDF font load failed: {e}")))?;
        let bold = doc
            .add_builtin_font(BuiltinFont::HelveticaBold)
            .map_err(|e| AppError::Internal(format!("PDF font load failed: {e}")))?;
        let layer = doc.get_page(page).get_layer(layer);
        Ok(Self { doc, layer, regular, bold, y: MARGIN_TOP })
    }

    /// Start a fresh page if drawing `needed` mm at the cursor would spill into
    /// the bottom margin. The cursor resets to the top of the new page.
    fn ensure_space(&mut self, needed: f32) {
        if self.y - needed < MARGIN_BOTTOM {
            let (page, layer) = self.doc.add_page(Mm(PAGE_W), Mm(PAGE_H), "Layer 1");
            self.layer = self.doc.get_page(page).get_layer(layer);
            self.y = MARGIN_TOP;
        }
    }

    /// Draw one line of text at the cursor, then advance past it. When `center`
    /// is set the line is centred in the content width; otherwise it starts at
    /// `x`. `bold` selects the font weight.
    fn line(&mut self, s: &str, size: f32, x: f32, bold: bool, center: bool) {
        self.ensure_space(line_h(size));
        let xx = if center {
            (MARGIN_L + (CONTENT_W - text_width_mm(s, size)) / 2.0).max(MARGIN_L)
        } else {
            x
        };
        let font = if bold { &self.bold } else { &self.regular };
        self.layer.use_text(s, size, Mm(xx), Mm(self.y), font);
        self.y -= line_h(size);
    }

    /// Word-wrap `text` to the page width (accounting for the left indent `x`)
    /// and draw each resulting line.
    fn paragraph(&mut self, text: &str, size: f32, x: f32) {
        let width = CONTENT_W - (x - MARGIN_L);
        for ln in wrap(text, size, width) {
            if ln.is_empty() {
                self.gap(line_h(size));
            } else {
                self.line(&ln, size, x, false, false);
            }
        }
    }

    /// A small bold heading followed by a wrapped body paragraph.
    fn section(&mut self, label: &str, body: &str) {
        self.line(label, SIZE_LABEL, MARGIN_L, true, false);
        self.gap(0.8);
        self.paragraph(body, SIZE_BODY, MARGIN_L);
        self.gap(2.5);
    }

    /// A full-width horizontal rule, drawn as a row of underscores so the whole
    /// generator stays on the stable `use_text` path (no vector-shape calls).
    fn rule(&mut self) {
        let count = (CONTENT_W / avg_char_mm(9.0)).floor() as usize;
        let bar = "_".repeat(count);
        self.ensure_space(line_h(9.0));
        self.layer.use_text(bar, 9.0, Mm(MARGIN_L), Mm(self.y), &self.regular);
        self.y -= line_h(9.0) * 0.5;
    }

    /// Drop the cursor by `mm` without drawing anything (vertical whitespace).
    fn gap(&mut self, mm: f32) {
        self.y -= mm;
    }

    /// Consume the builder and serialise the document to PDF bytes.
    fn finish(self) -> Result<Vec<u8>, AppError> {
        let mut buf = std::io::BufWriter::new(Vec::<u8>::new());
        self.doc
            .save(&mut buf)
            .map_err(|e| AppError::Internal(format!("PDF write failed: {e}")))?;
        buf.into_inner()
            .map_err(|e| AppError::Internal(format!("PDF buffer failed: {e}")))
    }
}

/// Render a printable medical report PDF from an assembled [`RecordReportData`].
///
/// The layout: a centred clinic letterhead, a two-column patient / attending-
/// doctor block, then the clinical sections (summary, diagnosis, treatment,
/// notes) and a prescriptions list, closing with a provenance footer.
pub fn render_record_report_pdf(report: &RecordReportData) -> Result<Vec<u8>, AppError> {
    let mut p = ReportPdf::new(&format!("Medical Report #{}", report.record.id))?;

    // --- Letterhead ---
    p.line("PatientMS Clinic", SIZE_TITLE, 0.0, true, true);
    p.line("Medical Report - Confidential", SIZE_SUBTITLE, 0.0, false, true);
    p.gap(1.5);
    p.rule();
    p.gap(3.0);

    // --- Two columns: patient (left) and attending doctor (right) ---
    let top = p.y;
    p.line("PATIENT", SIZE_LABEL, MARGIN_L, true, false);
    p.line(&report.patient_name, SIZE_VALUE, MARGIN_L, true, false);
    if let Some(dob) = report.patient_dob.as_deref() {
        p.line(&format!("Date of birth: {dob}"), SIZE_SMALL, MARGIN_L, false, false);
    }
    if let Some(bg) = report.patient_blood_group.as_deref() {
        p.line(&format!("Blood group: {bg}"), SIZE_SMALL, MARGIN_L, false, false);
    }
    let left_end = p.y;

    p.y = top;
    let col_r = MARGIN_L + CONTENT_W / 2.0;
    p.line("ATTENDING DOCTOR", SIZE_LABEL, col_r, true, false);
    p.line(&report.doctor_name, SIZE_VALUE, col_r, true, false);
    p.line(&report.doctor_specialization, SIZE_SMALL, col_r, false, false);
    let right_end = p.y;

    // Continue below whichever column ran longer.
    p.y = left_end.min(right_end);
    p.gap(3.0);
    p.rule();
    p.gap(3.0);

    // --- Clinical sections ---
    p.section("SUMMARY", &report.summary);
    p.section("DIAGNOSIS", report.record.diagnosis.as_deref().unwrap_or("-"));
    p.section("TREATMENT", report.record.treatment.as_deref().unwrap_or("-"));
    if let Some(notes) = report.record.notes.as_deref() {
        if !notes.trim().is_empty() {
            p.section("CLINICAL NOTES", notes);
        }
    }

    // --- Prescriptions (this consultation) ---
    if !report.prescriptions.is_empty() {
        p.line("PRESCRIPTIONS (this consultation)", SIZE_LABEL, MARGIN_L, true, false);
        p.gap(1.0);
        for (i, rx) in report.prescriptions.iter().enumerate() {
            let dur = rx
                .duration
                .as_deref()
                .map(|d| format!(" for {d}"))
                .unwrap_or_default();
            let text =
                format!("{}. {} - {}, {}{}", i + 1, rx.medication_name, rx.dosage, rx.frequency, dur);
            p.paragraph(&text, SIZE_BODY, MARGIN_L + 4.0);
        }
        p.gap(2.0);
    }

    // --- Footer / provenance ---
    p.gap(2.0);
    p.rule();
    p.gap(2.0);
    p.line(
        &format!("Record #{}   created {}", report.record.id, report.record.created_at),
        SIZE_SMALL,
        MARGIN_L,
        false,
        false,
    );
    p.line(&format!("Report generated {}", report.generated_at), SIZE_SMALL, MARGIN_L, false, false);

    p.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::records::models::MedicalRecord;

    fn sample_report() -> RecordReportData {
        RecordReportData {
            record: MedicalRecord {
                id: 7,
                patient_id: 1,
                doctor_id: 1,
                appointment_id: None,
                diagnosis: Some("Common cold".into()),
                treatment: Some("Rest and fluids".into()),
                notes: Some("Follow up in a week if symptoms persist.".into()),
                created_at: chrono::NaiveDate::from_ymd_opt(2026, 6, 1)
                    .unwrap()
                    .and_hms_opt(9, 0, 0)
                    .unwrap(),
            },
            summary: "Record #7 | Diagnosis: Common cold | Treatment: Rest and fluids".into(),
            patient_name: "John Doe".into(),
            patient_dob: Some("1990-01-01".into()),
            patient_blood_group: Some("O+".into()),
            doctor_name: "Dr. Smith".into(),
            doctor_specialization: "General Practice".into(),
            prescriptions: vec![],
            generated_at: "2026-06-28 12:00 UTC".into(),
        }
    }

    #[test]
    fn wrap_keeps_short_text_on_one_line() {
        assert_eq!(wrap("Common cold", SIZE_BODY, 170.0), vec!["Common cold".to_string()]);
    }

    #[test]
    fn wrap_splits_long_text_within_budget() {
        let text = "The quick brown fox jumps over the lazy dog and then runs around again";
        let lines = wrap(text, SIZE_BODY, 40.0);
        let max = ((40.0 / avg_char_mm(SIZE_BODY)).floor() as usize).max(8);
        assert!(lines.len() > 1, "narrow width should wrap onto multiple lines");
        assert!(lines.iter().all(|l| l.chars().count() <= max), "no line exceeds the budget");
    }

    #[test]
    fn wrap_hard_breaks_an_unbroken_word() {
        let long = "A".repeat(200);
        let lines = wrap(&long, SIZE_BODY, 40.0);
        assert!(lines.len() > 1, "a single huge word must be hard-broken");
    }

    #[test]
    fn renders_real_pdf_bytes() {
        let bytes = render_record_report_pdf(&sample_report()).expect("render should succeed");
        // The PDF magic number proves we produced a genuine PDF, not HTML.
        assert!(bytes.starts_with(b"%PDF"), "output must begin with the %PDF header");
        assert!(bytes.len() > 400, "a real report carries more than a few bytes");
    }

    #[test]
    fn renders_with_prescriptions_and_long_notes() {
        use crate::records::models::Prescription;
        let mut report = sample_report();
        report.record.notes = Some("word ".repeat(400)); // forces pagination
        report.prescriptions = vec![Prescription {
            id: 1,
            patient_id: 1,
            doctor_id: 1,
            appointment_id: None,
            medication_name: "Amoxicillin".into(),
            dosage: "500mg".into(),
            frequency: "3x daily".into(),
            duration: Some("7 days".into()),
            notes: None,
            prescribed_at: chrono::NaiveDate::from_ymd_opt(2026, 6, 1)
                .unwrap()
                .and_hms_opt(9, 0, 0)
                .unwrap(),
        }];
        let bytes = render_record_report_pdf(&report).expect("render should succeed");
        assert!(bytes.starts_with(b"%PDF"));
    }
}
