use craft_archive::{
    ExtractionPolicy, ResourceBudget, create_simple_zip, extract_archive,
    extract_archive_with_report, inspect_bytes,
};
use tempfile::tempdir;

#[test]
fn eval_zip_slip_strict_aborts() {
    let budget = ResourceBudget::default();
    let tmp = tempdir().expect("tempdir");
    let dest = tmp.path().join("out");

    // Construct a ZIP archive containing a Zip Slip directory traversal
    let payload = create_simple_zip(&[
        ("valid.txt", b"Safe payload content"),
        ("../../evil.sh", b"#!/bin/sh\nrm -rf /"),
    ])
    .expect("create zip");

    let zip_path = tmp.path().join("slip.zip");
    std::fs::write(&zip_path, payload).expect("write zip");

    let result = extract_archive(&zip_path, &dest, &budget);
    assert!(
        result.is_err(),
        "Strict extraction must abort on traversal attempt"
    );

    // Ensure the evil file was never created anywhere
    assert!(!tmp.path().join("evil.sh").exists());
}

#[test]
fn eval_zip_slip_permissive_skips_and_reports() {
    let budget = ResourceBudget::default();
    let tmp = tempdir().expect("tempdir");
    let dest = tmp.path().join("out");

    let payload = create_simple_zip(&[
        ("safe1.txt", b"Safe file 1"),
        ("../../etc_passwd.txt", b"root:x:0:0:root:..."),
        ("sub/safe2.txt", b"Safe file 2"),
    ])
    .expect("create zip");

    let zip_path = tmp.path().join("permissive.zip");
    std::fs::write(&zip_path, payload).expect("write zip");

    let report =
        extract_archive_with_report(&zip_path, &dest, &budget, ExtractionPolicy::Permissive)
            .expect("permissive extraction succeeds");

    assert_eq!(report.extracted_paths.len(), 2);
    assert_eq!(report.skipped_entries.len(), 1);
    assert_eq!(report.skipped_entries[0].raw_name, "../../etc_passwd.txt");
    assert!(!report.warnings.is_empty());

    assert!(dest.join("safe1.txt").exists());
    assert!(dest.join("sub/safe2.txt").exists());
    assert!(!tmp.path().join("etc_passwd.txt").exists());
}

#[test]
fn eval_decompression_bomb_single_entry_bounded() {
    let budget = ResourceBudget {
        max_single_entry_bytes: 1024, // 1 KB max
        ..Default::default()
    };

    let tmp = tempdir().expect("tempdir");
    let dest = tmp.path().join("out");

    // 64 KB of repeated data compresses to ~100 bytes
    let huge_data = vec![b'A'; 64 * 1024];
    let payload = create_simple_zip(&[("bomb.txt", &huge_data)]).expect("create zip");

    let zip_path = tmp.path().join("bomb.zip");
    std::fs::write(&zip_path, payload).expect("write zip");

    let result = extract_archive(&zip_path, &dest, &budget);
    assert!(result.is_err(), "Single entry bomb must breach 1 KB budget");

    // Ensure oversized file is scrubbed
    assert!(!dest.join("bomb.txt").exists());
}

#[test]
fn eval_decompression_bomb_aggregate_total_bounded() {
    let budget = ResourceBudget {
        max_single_entry_bytes: 2048,
        max_total_uncompressed_bytes: 3000, // 3 KB aggregate cap
        ..Default::default()
    };

    let tmp = tempdir().expect("tempdir");
    let dest = tmp.path().join("out");

    let part1 = vec![b'B'; 1500];
    let part2 = vec![b'C'; 1800]; // 1500 + 1800 = 3300 > 3000

    let payload =
        create_simple_zip(&[("file1.dat", &part1), ("file2.dat", &part2)]).expect("create zip");

    let zip_path = tmp.path().join("aggregate_bomb.zip");
    std::fs::write(&zip_path, payload).expect("write zip");

    let result = extract_archive(&zip_path, &dest, &budget);
    assert!(
        result.is_err(),
        "Aggregate uncompressed size must breach 3 KB limit"
    );
}

#[test]
fn eval_entry_count_limit_enforced() {
    let budget = ResourceBudget {
        max_entries: 3,
        ..Default::default()
    };

    let tmp = tempdir().expect("tempdir");
    let dest = tmp.path().join("out");

    let payload = create_simple_zip(&[
        ("f1.txt", b"1"),
        ("f2.txt", b"2"),
        ("f3.txt", b"3"),
        ("f4.txt", b"4"),
    ])
    .expect("create zip");

    let zip_path = tmp.path().join("too_many.zip");
    std::fs::write(&zip_path, payload).expect("write zip");

    let result = extract_archive(&zip_path, &dest, &budget);
    assert!(
        result.is_err(),
        "Archive with 4 entries must breach 3 entry limit"
    );
}

#[test]
fn eval_tar_slip_prevention() {
    let budget = ResourceBudget::default();
    let tmp = tempdir().expect("tempdir");
    let dest = tmp.path().join("out");

    let mut tar_buf = vec![0u8; 1536]; // 512 header + 512 data + 512 EOF
    let name = b"../../evil_tar.sh";
    tar_buf[..name.len()].copy_from_slice(name);
    tar_buf[100..108].copy_from_slice(b"0000755\0");
    tar_buf[124..136].copy_from_slice(b"00000000014\0"); // 12 bytes
    tar_buf[257..263].copy_from_slice(b"ustar\0");
    tar_buf[263..265].copy_from_slice(b"00");

    let mut sum = 8 * (b' ' as u32);
    for (i, &byte) in tar_buf[..512].iter().enumerate() {
        if !(148..156).contains(&i) {
            sum = sum.saturating_add(byte as u32);
        }
    }
    let chk = format!("{sum:06o}\0 ");
    tar_buf[148..156].copy_from_slice(chk.as_bytes());
    tar_buf[512..524].copy_from_slice(b"#!/bin/sh\nls");

    let tar_path = tmp.path().join("slip.tar");
    std::fs::write(&tar_path, tar_buf).expect("write tar");

    let result = extract_archive(&tar_path, &dest, &budget);
    assert!(result.is_err(), "Tar path traversal must be blocked");
    assert!(!tmp.path().join("evil_tar.sh").exists());
}

#[test]
fn eval_inspect_bytes_diagnostic_receipt() {
    let budget = ResourceBudget::default();
    let payload = create_simple_zip(&[
        ("manifest.json", b"{\"version\": 1}"),
        ("assets/preview.png", &[137, 80, 78, 71, 13, 10, 26, 10]),
    ])
    .expect("create zip");

    let summary = inspect_bytes(&payload, &budget).expect("inspect zip bytes");
    assert_eq!(summary.entry_count(), 2);
    assert_eq!(summary.total_uncompressed_bytes, 22);
    assert_eq!(summary.format.as_str(), "zip");
    assert_eq!(summary.entries.len(), 2);
    assert_eq!(summary.entries[0].name, "manifest.json");
    assert_eq!(summary.entries[1].name, "assets/preview.png");
}
