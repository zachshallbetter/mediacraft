use craft_pdf::{Dict, PdfInspector, Stream};

#[test]
fn eval_garbage_bytes_rejected_without_panic() {
    let garbage = b"THIS IS NOT A VALID PDF FILE AT ALL";
    assert!(PdfInspector::open(garbage).is_err());

    let empty = b"";
    assert!(PdfInspector::open(empty).is_err());

    let header_only = b"%PDF-1.7\n";
    assert!(PdfInspector::open(header_only).is_err());
}

#[test]
fn eval_valid_minimal_pdf_diagnostics() {
    let pdf_bytes = b"%PDF-1.4\n\
1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n\
2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n\
3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] >>\nendobj\n\
4 0 obj\n<< /Title (Deliverables Agreement) /Author (Legal Team) >>\nendobj\n\
xref\n\
0 5\n\
0000000000 65535 f \n\
0000000009 00000 n \n\
0000000058 00000 n \n\
0000000115 00000 n \n\
0000000190 00000 n \n\
trailer\n<< /Size 5 /Root 1 0 R /Info 4 0 R >>\n\
startxref\n260\n%%EOF\n";

    let inspector = PdfInspector::open(pdf_bytes).expect("open valid minimal pdf");
    assert_eq!(inspector.page_count(), 1);
    assert_eq!(inspector.version(), "1.4");
    assert_eq!(inspector.title().as_deref(), Some("Deliverables Agreement"));
    assert_eq!(inspector.author().as_deref(), Some("Legal Team"));
    assert_eq!(inspector.dimensions(0), Some([0.0, 0.0, 612.0, 792.0]));

    let report = inspector.diagnose();
    assert_eq!(report.page_count_declared, 1);
    assert_eq!(report.page_count_actual, 1);
    assert!(!report.is_encrypted);
    assert!(report.warnings.is_empty());
    assert_eq!(report.page_dimensions.len(), 1);
}

#[test]
fn eval_declared_vs_actual_page_count_mismatch_warning() {
    // Deliberately set /Count 5 when only 1 page actually exists
    let pdf_bytes = b"%PDF-1.7\n\
1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n\
2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 5 >>\nendobj\n\
3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] >>\nendobj\n\
xref\n\
0 4\n\
0000000000 65535 f \n\
0000000009 00000 n \n\
0000000058 00000 n \n\
0000000115 00000 n \n\
trailer\n<< /Size 4 /Root 1 0 R >>\n\
startxref\n190\n%%EOF\n";

    let inspector = PdfInspector::open(pdf_bytes).expect("open pdf with count mismatch");
    assert_eq!(inspector.page_count(), 5); // declared

    let report = inspector.diagnose();
    assert_eq!(report.page_count_declared, 5);
    assert_eq!(report.page_count_actual, 1);
    assert_eq!(report.warnings.len(), 1);
    assert!(report.warnings[0].contains("differs from actual reachable pages"));
}

#[test]
fn eval_damaged_xref_offset_reconstructed_by_byte_scan() {
    // startxref points to 999999 (invalid), forced to reconstruct xref
    let pdf_bytes = b"%PDF-1.4\n\
1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n\
2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n\
3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 100] >>\nendobj\n\
trailer\n<< /Size 4 /Root 1 0 R >>\n\
startxref\n999999\n%%EOF\n";

    let inspector = PdfInspector::open(pdf_bytes).expect("damaged xref should recover via scan");
    assert_eq!(inspector.page_count(), 1);
    assert_eq!(inspector.dimensions(0), Some([0.0, 0.0, 100.0, 100.0]));
}

#[test]
fn eval_decompression_bomb_stream_bound() {
    // Create a flate compressed stream that expands drastically
    let decompressed_pattern = vec![b'Z'; 64 * 1024]; // 64 KB
    let stream = Stream::flate(Dict::new(), &decompressed_pattern);

    // When requested with caller limit of 1024 bytes, decoding beyond limit is refused
    let decoded = stream.decoded_within(1024);
    assert!(
        decoded.is_err(),
        "Caller limit of 1024 bytes must stop 64 KB decompression"
    );
}
