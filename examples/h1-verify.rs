use std::{env, fs, path::Path, process::ExitCode};

const MAX_ARTIFACT_BYTES: u64 = 1024 * 1024;

fn read_bounded(path: &Path) -> Result<String, ()> {
    let metadata = path.metadata().map_err(|_| ())?;
    if !metadata.is_file() || metadata.len() > MAX_ARTIFACT_BYTES {
        return Err(());
    }
    fs::read_to_string(path).map_err(|_| ())
}

fn disposition(value: bool) -> &'static str {
    if value {
        "PASS"
    } else {
        "FAIL"
    }
}

fn main() -> ExitCode {
    let mut arguments = env::args_os().skip(1);
    let mut binding = None;
    let mut workflow = None;
    while let Some(argument) = arguments.next() {
        match argument.to_str() {
            Some("--binding") if binding.is_none() => binding = arguments.next(),
            Some("--workflow") if workflow.is_none() => workflow = arguments.next(),
            _ => {
                eprintln!("H1_ARTIFACT_VERIFIER=FAIL_ARGUMENT");
                return ExitCode::from(2);
            }
        }
    }
    let (Some(binding), Some(workflow)) = (binding, workflow) else {
        eprintln!("H1_ARTIFACT_VERIFIER=FAIL_ARGUMENT");
        return ExitCode::from(2);
    };
    let (Ok(binding), Ok(workflow)) = (
        read_bounded(Path::new(&binding)),
        read_bounded(Path::new(&workflow)),
    ) else {
        eprintln!("H1_ARTIFACT_VERIFIER=FAIL_READ");
        return ExitCode::from(3);
    };
    let assessment = runnermesh::assess_h1_artifacts(&binding, &workflow);
    println!("EVIDENCE_SCOPE=H1_ARTIFACTS_READ_ONLY");
    println!(
        "BINDING_SCHEMA={}",
        disposition(assessment.binding_schema_valid)
    );
    println!(
        "BINDING_PLACEHOLDERS_ABSENT={}",
        disposition(assessment.placeholders_absent)
    );
    println!(
        "BINDING_SEMANTICS={}",
        disposition(assessment.binding_semantics_valid)
    );
    println!(
        "WORKFLOW_SOURCE_CONTRACT={}",
        disposition(assessment.workflow_source_contract_ready)
    );
    println!("H1_ARTIFACTS_READY={}", disposition(assessment.ready()));
    println!("LIVE_READINESS_EXECUTED=false");
    println!("H1_MUTATION_ALLOWED=false");
    if assessment.ready() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}
