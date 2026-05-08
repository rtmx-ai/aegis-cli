// rtmx:req REQ-TEST-048
// rtmx:req REQ-TEST-049
//! Criterion benchmark suite for aegis-cli hot paths.
//!
//! Covers three hot paths:
//! 1. RTM CSV parse (RequirementsDb::from_csv)
//! 2. Audit ledger append (JsonlLedger::record)
//! 3. Tool dispatch (ToolCall::risk classification)

use criterion::{Criterion, black_box, criterion_group, criterion_main};

// ---------------------------------------------------------------------------
// 1. RTM CSV parse benchmark (REQ-TEST-049)
// ---------------------------------------------------------------------------

fn build_csv_fixture(num_rows: usize) -> String {
    let mut csv = String::from(
        "req_id,category,subcategory,requirement_text,target_value,\
         test_module,test_function,validation_method,status,priority,\
         phase,notes,effort_weeks,dependencies,blocks,assignee,sprint,\
         started_date,completed_date\n",
    );
    for i in 0..num_rows {
        csv.push_str(&format!(
            "REQ-BENCH-{i:04},BENCH,SUB,Requirement {i} text,target {i},\
             mod_{i},fn_{i},Unit Test,DRAFT,MEDIUM,1,notes {i},1,,,,,,,\n",
        ));
    }
    csv
}

fn bench_rtm_csv_parse(c: &mut Criterion) {
    let mut group = c.benchmark_group("rtm_csv_parse");

    // Small DB (~10 rows)
    let small_csv = build_csv_fixture(10);
    group.bench_function("10_rows", |b| {
        b.iter(|| {
            let db = aegis_domain::rtmx::RequirementsDb::from_csv(black_box(&small_csv));
            black_box(db).unwrap();
        });
    });

    // Medium DB (~160 rows, similar to real database)
    let medium_csv = build_csv_fixture(160);
    group.bench_function("160_rows", |b| {
        b.iter(|| {
            let db = aegis_domain::rtmx::RequirementsDb::from_csv(black_box(&medium_csv));
            black_box(db).unwrap();
        });
    });

    // Large DB (~1000 rows, stress test)
    let large_csv = build_csv_fixture(1000);
    group.bench_function("1000_rows", |b| {
        b.iter(|| {
            let db = aegis_domain::rtmx::RequirementsDb::from_csv(black_box(&large_csv));
            black_box(db).unwrap();
        });
    });

    group.finish();
}

// ---------------------------------------------------------------------------
// 2. Audit ledger append benchmark (REQ-TEST-049)
// ---------------------------------------------------------------------------

fn make_session_started_event() -> aegis_domain::event::DomainEvent {
    aegis_domain::event::DomainEvent::SessionStarted {
        session_id: aegis_domain::types::SessionId::new(),
        timestamp: chrono::Utc::now(),
    }
}

fn bench_audit_ledger_append(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let mut group = c.benchmark_group("audit_ledger_append");

    // Benchmark appending 100 events to a fresh ledger.
    group.bench_function("100_events", |b| {
        b.iter(|| {
            rt.block_on(async {
                let tmp = tempfile::TempDir::new().unwrap();
                let ledger = aegis_audit::ledger::JsonlLedger::new(tmp.path())
                    .await
                    .unwrap();
                let event = make_session_started_event();
                for _ in 0..100 {
                    use aegis_domain::ports::AuditLedger;
                    ledger.record(black_box(&event)).await.unwrap();
                }
            });
        });
    });

    // Single-event append (amortized overhead)
    group.bench_function("single_event", |b| {
        let tmp = tempfile::TempDir::new().unwrap();
        let ledger = rt
            .block_on(aegis_audit::ledger::JsonlLedger::new(tmp.path()))
            .unwrap();
        let event = make_session_started_event();
        b.iter(|| {
            rt.block_on(async {
                use aegis_domain::ports::AuditLedger;
                ledger.record(black_box(&event)).await.unwrap();
            });
        });
    });

    group.finish();
}

// ---------------------------------------------------------------------------
// 3. Tool dispatch / risk classification benchmark (REQ-TEST-049)
// ---------------------------------------------------------------------------

fn bench_tool_dispatch(c: &mut Criterion) {
    use aegis_domain::types::{FilePath, ToolCall, ToolRisk};

    let mut group = c.benchmark_group("tool_dispatch");

    // Build a representative set of tool calls.
    let tool_calls: Vec<ToolCall> = vec![
        ToolCall::ReadFile {
            path: FilePath::new_unchecked("src/main.rs"),
        },
        ToolCall::WriteFile {
            path: FilePath::new_unchecked("output.txt"),
            content: "hello world".to_string(),
        },
        ToolCall::RunCommand {
            command: "echo hello".to_string(),
            timeout_secs: 30,
        },
        ToolCall::ListDir {
            path: FilePath::new_unchecked("."),
        },
        ToolCall::Grep {
            pattern: "fn main".to_string(),
            path: FilePath::new_unchecked("src/"),
        },
        ToolCall::McpTool {
            qualified_name: "server__tool".to_string(),
            arguments: serde_json::json!({"key": "value"}),
        },
    ];

    // Benchmark risk classification across all tool variants.
    group.bench_function("risk_classification", |b| {
        b.iter(|| {
            for tc in &tool_calls {
                let risk = black_box(tc).risk();
                black_box(risk);
            }
        });
    });

    // Benchmark the match dispatch pattern (ReadOnly vs StateMutating).
    group.bench_function("risk_is_read_only_check", |b| {
        b.iter(|| {
            for tc in &tool_calls {
                let is_readonly = black_box(tc).risk() == ToolRisk::ReadOnly;
                black_box(is_readonly);
            }
        });
    });

    // Benchmark constructing a ToolCall (allocation cost).
    group.bench_function("construct_grep_call", |b| {
        b.iter(|| {
            let tc = ToolCall::Grep {
                pattern: black_box("fn\\s+\\w+").to_string(),
                path: FilePath::new_unchecked(black_box("src/")),
            };
            black_box(tc);
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_rtm_csv_parse,
    bench_audit_ledger_append,
    bench_tool_dispatch,
);
criterion_main!(benches);
