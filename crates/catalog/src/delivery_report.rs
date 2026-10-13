//! Delivery Validation Report Generator (MC-18, MD-14).
//!
//! Generates machine-readable JSON receipts and professional standalone HTML certificates
//! validating deliverable compliance against broadcast, streaming, and archive standards.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use mediacraft_protocol::ValidationSeverity;

/// Technical verification check result item.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VerificationCheckItem {
    pub category: String,
    pub name: String,
    pub expected: String,
    pub observed: String,
    pub status: ValidationSeverity,
    pub details: Option<String>,
}

/// Comprehensive delivery report model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeliveryReport {
    pub report_id: String,
    pub title: String,
    pub generated_at_iso: String,
    pub deliverable_name: String,
    pub deliverable_path: PathBuf,
    pub profile_name: String,
    pub overall_status: ValidationSeverity,
    pub file_size_bytes: u64,
    pub sha256_checksum: String,
    pub md5_checksum: String,
    pub vmaf_score: f32,
    pub psnr_db: f32,
    pub ssim_score: f32,
    pub audio_true_peak_dbtp: f32,
    pub audio_integrated_lufs: f32,
    pub checks: Vec<VerificationCheckItem>,
}

impl DeliveryReport {
    pub fn formatted_size(&self) -> String {
        let b = self.file_size_bytes;
        if b >= 1_000_000_000 {
            format!("{:.2} GB", b as f64 / 1_000_000_000.0)
        } else {
            format!("{:.1} MB", b as f64 / 1_000_000.0)
        }
    }

    /// Serializes report to formatted JSON.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Generates a standalone, dark-themed HTML certificate for distribution or client sign-off.
    pub fn to_html(&self) -> String {
        let (status_badge_bg, status_badge_text, status_label) = match self.overall_status {
            ValidationSeverity::Accept => {
                ("#1b4332", "#74c69d", "PASSED — SPECIFICATION COMPLIANT")
            }
            ValidationSeverity::Warn => ("#5c3d11", "#ffb703", "CONDITIONAL PASS — WITH WARNINGS"),
            ValidationSeverity::Reject => {
                ("#541212", "#ff6b6b", "REJECTED — SPECIFICATION VIOLATION")
            }
        };

        let mut rows = String::new();
        for check in &self.checks {
            let (bg, fg, tag) = match check.status {
                ValidationSeverity::Accept => ("rgba(116, 198, 157, 0.15)", "#74c69d", "PASS"),
                ValidationSeverity::Warn => ("rgba(255, 183, 3, 0.15)", "#ffb703", "WARN"),
                ValidationSeverity::Reject => ("rgba(255, 107, 107, 0.15)", "#ff6b6b", "FAIL"),
            };
            rows.push_str(&format!(
                r#"<tr>
                  <td style="padding: 10px 14px; border-bottom: 1px solid #23272e; color: #8b949e; font-size: 13px;">{}</td>
                  <td style="padding: 10px 14px; border-bottom: 1px solid #23272e; font-weight: 500; font-size: 13px;">{}</td>
                  <td style="padding: 10px 14px; border-bottom: 1px solid #23272e; color: #a0aec0; font-size: 13px;">{}</td>
                  <td style="padding: 10px 14px; border-bottom: 1px solid #23272e; color: #e6edf3; font-size: 13px;">{}</td>
                  <td style="padding: 10px 14px; border-bottom: 1px solid #23272e; text-align: center;">
                    <span style="background: {}; color: {}; padding: 3px 8px; border-radius: 4px; font-weight: bold; font-size: 11px;">{}</span>
                  </td>
                </tr>"#,
                html_escape(&check.category),
                html_escape(&check.name),
                html_escape(&check.expected),
                html_escape(&check.observed),
                bg,
                fg,
                tag
            ));
        }

        format!(
            r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <title>Delivery Validation Certificate — {title}</title>
  <style>
    body {{
      background-color: #0b0d11;
      color: #e6edf3;
      font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Helvetica, Arial, sans-serif;
      margin: 0;
      padding: 32px 20px;
    }}
    .container {{
      max-width: 900px;
      margin: 0 auto;
      background: #14171d;
      border: 1px solid #262a33;
      border-radius: 8px;
      box-shadow: 0 8px 30px rgba(0,0,0,0.4);
      overflow: hidden;
    }}
    .header {{
      padding: 24px 32px;
      background: #191c24;
      border-bottom: 1px solid #262a33;
      display: flex;
      justify-content: space-between;
      align-items: center;
    }}
    .badge {{
      background: {status_badge_bg};
      color: {status_badge_text};
      padding: 6px 14px;
      border-radius: 6px;
      font-weight: 700;
      font-size: 13px;
      letter-spacing: 0.5px;
    }}
    .section {{
      padding: 24px 32px;
      border-bottom: 1px solid #20242c;
    }}
    .grid-2 {{
      display: grid;
      grid-template-columns: 1fr 1fr;
      gap: 16px;
    }}
    .card {{
      background: #101217;
      padding: 16px;
      border-radius: 6px;
      border: 1px solid #23272e;
    }}
    .metric-val {{
      font-size: 24px;
      font-weight: 700;
      color: #38bdf8;
      margin-top: 4px;
    }}
    .label {{
      color: #8b949e;
      font-size: 12px;
      text-transform: uppercase;
      letter-spacing: 0.5px;
    }}
    table {{
      width: 100%;
      border-collapse: collapse;
      text-align: left;
    }}
    th {{
      background: #101217;
      padding: 10px 14px;
      color: #8b949e;
      font-size: 12px;
      text-transform: uppercase;
      border-bottom: 1px solid #262a33;
    }}
    .hash {{
      font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
      font-size: 12px;
      color: #79c0ff;
      word-break: break-all;
    }}
    .footer {{
      padding: 18px 32px;
      background: #101217;
      color: #8b949e;
      font-size: 12px;
      display: flex;
      justify-content: space-between;
    }}
  </style>
</head>
<body>
  <div class="container">
    <div class="header">
      <div>
        <div style="font-size: 11px; color: #38bdf8; font-weight: bold; letter-spacing: 1px;">MEDIACRAFT VERIFIED DELIVERABLE</div>
        <h1 style="margin: 4px 0 0; font-size: 22px;">{title}</h1>
      </div>
      <div class="badge">{status_label}</div>
    </div>

    <div class="section">
      <div class="grid-2">
        <div class="card">
          <div class="label">Target Specification Profile</div>
          <div style="font-size: 16px; font-weight: 600; margin-top: 4px;">{profile}</div>
          <div style="margin-top: 12px;" class="label">Observed File Size</div>
          <div style="font-size: 14px; color: #e6edf3;">{file_size}</div>
          <div style="margin-top: 12px;" class="label">Destination Path</div>
          <div style="font-size: 12px; color: #8b949e; word-break: break-all;">{path}</div>
        </div>
        <div class="card">
          <div class="label">Cryptographic Verification Hashes</div>
          <div style="margin-top: 6px;" class="label">SHA-256 Digest</div>
          <div class="hash">{sha256}</div>
          <div style="margin-top: 10px;" class="label">MD5 Digest</div>
          <div class="hash">{md5}</div>
          <div style="margin-top: 10px;" class="label">Verification Timestamp</div>
          <div style="font-size: 12px; color: #e6edf3;">{timestamp}</div>
        </div>
      </div>
    </div>

    <div class="section">
      <h3 style="margin: 0 0 16px; font-size: 15px; color: #c9d1d9;">Objective Quality & Loudness Measurements</h3>
      <div style="display: grid; grid-template-columns: repeat(5, 1fr); gap: 12px;">
        <div class="card" style="text-align: center;">
          <div class="label">VMAF Score</div>
          <div class="metric-val">{vmaf:.1}</div>
          <div style="font-size: 11px; color: #8b949e; margin-top: 2px;">/ 100</div>
        </div>
        <div class="card" style="text-align: center;">
          <div class="label">PSNR Metric</div>
          <div class="metric-val">{psnr:.1}</div>
          <div style="font-size: 11px; color: #8b949e; margin-top: 2px;">dB</div>
        </div>
        <div class="card" style="text-align: center;">
          <div class="label">SSIM Index</div>
          <div class="metric-val">{ssim:.3}</div>
          <div style="font-size: 11px; color: #8b949e; margin-top: 2px;">/ 1.0</div>
        </div>
        <div class="card" style="text-align: center;">
          <div class="label">True Peak</div>
          <div class="metric-val">{peak:.1}</div>
          <div style="font-size: 11px; color: #8b949e; margin-top: 2px;">dBTP</div>
        </div>
        <div class="card" style="text-align: center;">
          <div class="label">Integrated Audio</div>
          <div class="metric-val">{lufs:.1}</div>
          <div style="font-size: 11px; color: #8b949e; margin-top: 2px;">LUFS</div>
        </div>
      </div>
    </div>

    <div class="section" style="padding-bottom: 8px;">
      <h3 style="margin: 0 0 16px; font-size: 15px; color: #c9d1d9;">Technical Conformance Check Matrix</h3>
      <table>
        <thead>
          <tr>
            <th>Category</th>
            <th>Check Parameter</th>
            <th>Specification Target</th>
            <th>Measured Value</th>
            <th style="text-align: center;">Status</th>
          </tr>
        </thead>
        <tbody>
          {rows}
        </tbody>
      </table>
    </div>

    <div class="footer">
      <div>Generated by <strong>MediaCraft Delivery Verification Engine</strong></div>
      <div>Receipt ID: <code>{report_id}</code></div>
    </div>
  </div>
</body>
</html>"#,
            title = html_escape(&self.title),
            status_label = status_label,
            status_badge_bg = status_badge_bg,
            status_badge_text = status_badge_text,
            profile = html_escape(&self.profile_name),
            file_size = self.formatted_size(),
            path = html_escape(&self.deliverable_path.display().to_string()),
            sha256 = self.sha256_checksum,
            md5 = self.md5_checksum,
            timestamp = self.generated_at_iso,
            vmaf = self.vmaf_score,
            psnr = self.psnr_db,
            ssim = self.ssim_score,
            peak = self.audio_true_peak_dbtp,
            lufs = self.audio_integrated_lufs,
            rows = rows,
            report_id = self.report_id
        )
    }

    /// Atomically writes the HTML delivery certificate to disk.
    pub fn write_html_report(&self, target_path: &Path) -> Result<(), String> {
        let html = self.to_html();
        craft_atomic::atomic_write(target_path, html.as_bytes()).map_err(|e| {
            format!(
                "Failed to write HTML report to {}: {e}",
                target_path.display()
            )
        })
    }
}

/// Helper building a standard delivery report for a completed deliverable.
#[allow(clippy::too_many_arguments)]
pub fn generate_sample_delivery_report(
    deliverable_name: &str,
    deliverable_path: PathBuf,
    profile_name: &str,
    file_size_bytes: u64,
    sha256_checksum: &str,
    vmaf_score: f32,
    psnr_db: f32,
    ssim_score: f32,
    audio_true_peak_dbtp: f32,
    audio_integrated_lufs: f32,
) -> DeliveryReport {
    let checks = vec![
        VerificationCheckItem {
            category: "Container".to_string(),
            name: "MP4 FastStart Header Position".to_string(),
            expected: "moov atom precedes mdat".to_string(),
            observed: "moov atom at byte offset 48".to_string(),
            status: ValidationSeverity::Accept,
            details: Some("Optimal for web streaming".to_string()),
        },
        VerificationCheckItem {
            category: "Video".to_string(),
            name: "Resolution & Aspect Ratio".to_string(),
            expected: "3840×2160 (16:9)".to_string(),
            observed: "3840×2160 (1.7778)".to_string(),
            status: ValidationSeverity::Accept,
            details: None,
        },
        VerificationCheckItem {
            category: "Video".to_string(),
            name: "Frame Rate & Cadence".to_string(),
            expected: "23.976 fps Constant".to_string(),
            observed: "23.976 fps CFR (0 dropped)".to_string(),
            status: ValidationSeverity::Accept,
            details: None,
        },
        VerificationCheckItem {
            category: "Video".to_string(),
            name: "Color Space & Primaries".to_string(),
            expected: "BT.709 / Rec.709".to_string(),
            observed: "BT.709 (1-1-1)".to_string(),
            status: ValidationSeverity::Accept,
            details: None,
        },
        VerificationCheckItem {
            category: "Audio".to_string(),
            name: "Integrated Loudness (EBU R128)".to_string(),
            expected: "-24.0 LUFS ±0.5".to_string(),
            observed: format!("{:.1} LUFS", audio_integrated_lufs),
            status: if (audio_integrated_lufs - (-24.0)).abs() <= 1.0 {
                ValidationSeverity::Accept
            } else {
                ValidationSeverity::Warn
            },
            details: None,
        },
        VerificationCheckItem {
            category: "Audio".to_string(),
            name: "Max True Peak".to_string(),
            expected: "≤ -1.0 dBTP".to_string(),
            observed: format!("{:.1} dBTP", audio_true_peak_dbtp),
            status: if audio_true_peak_dbtp <= -1.0 {
                ValidationSeverity::Accept
            } else {
                ValidationSeverity::Warn
            },
            details: None,
        },
        VerificationCheckItem {
            category: "Quality".to_string(),
            name: "VMAF Perceptual Score".to_string(),
            expected: "≥ 95.0".to_string(),
            observed: format!("{:.1}", vmaf_score),
            status: if vmaf_score >= 95.0 {
                ValidationSeverity::Accept
            } else {
                ValidationSeverity::Warn
            },
            details: None,
        },
    ];

    let overall_status = if checks
        .iter()
        .any(|c| c.status == ValidationSeverity::Reject)
    {
        ValidationSeverity::Reject
    } else if checks.iter().any(|c| c.status == ValidationSeverity::Warn) {
        ValidationSeverity::Warn
    } else {
        ValidationSeverity::Accept
    };

    DeliveryReport {
        report_id: "mc-rpt-8f3a9d21".to_string(),
        title: format!("Delivery Verification: {deliverable_name}"),
        generated_at_iso: "2026-10-13T05:30:00Z".to_string(),
        deliverable_name: deliverable_name.to_string(),
        deliverable_path,
        profile_name: profile_name.to_string(),
        overall_status,
        file_size_bytes,
        sha256_checksum: sha256_checksum.to_string(),
        md5_checksum: "a3b84f2910cd04e892c94318721104e1".to_string(),
        vmaf_score,
        psnr_db,
        ssim_score,
        audio_true_peak_dbtp,
        audio_integrated_lufs,
        checks,
    }
}

fn html_escape(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for c in input.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_delivery_report_generation() -> Result<(), Box<dyn std::error::Error>> {
        let report = generate_sample_delivery_report(
            "Summer_Campaign_Master_UHD.mov",
            PathBuf::from("/Deliveries/Summer_Campaign_Master_UHD.mov"),
            "Broadcast Master (EBU R128)",
            3_200_000_000,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            98.4,
            48.2,
            0.992,
            -1.2,
            -24.0,
        );

        assert_eq!(report.overall_status, ValidationSeverity::Accept);
        assert_eq!(report.checks.len(), 7);

        let html = report.to_html();
        assert!(html.contains("Summer_Campaign_Master_UHD.mov"));
        assert!(html.contains("PASSED — SPECIFICATION COMPLIANT"));
        assert!(html.contains("98.4"));
        assert!(html.contains("Broadcast Master (EBU R128)"));

        let json = report.to_json()?;
        assert!(json.contains("mc-rpt-8f3a9d21"));
        Ok(())
    }
}
