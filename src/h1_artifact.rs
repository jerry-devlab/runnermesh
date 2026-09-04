use serde::{Deserialize, Serialize};

use crate::{assess_h1_workflow_source, H1LiveBinding};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct H1ArtifactAssessment {
    pub binding_schema_valid: bool,
    pub placeholders_absent: bool,
    pub binding_semantics_valid: bool,
    pub workflow_source_contract_ready: bool,
}

impl H1ArtifactAssessment {
    pub fn ready(self) -> bool {
        self.binding_schema_valid
            && self.placeholders_absent
            && self.binding_semantics_valid
            && self.workflow_source_contract_ready
    }
}

/// Deterministically assesses secret-free H1 artifacts without performing
/// credential, network, filesystem-metadata, process, runner, or host I/O.
pub fn assess_h1_artifacts(binding_source: &str, workflow_source: &str) -> H1ArtifactAssessment {
    let placeholders_absent = !binding_source.to_ascii_uppercase().contains("REPLACE_WITH");
    let binding = serde_json::from_str::<H1LiveBinding>(binding_source);
    let binding_schema_valid = binding.is_ok();
    let binding_semantics_valid =
        placeholders_absent && binding.as_ref().is_ok_and(H1LiveBinding::is_valid);
    H1ArtifactAssessment {
        binding_schema_valid,
        placeholders_absent,
        binding_semantics_valid,
        workflow_source_contract_ready: assess_h1_workflow_source(workflow_source)
            .source_contract_ready(),
    }
}

#[cfg(test)]
mod tests {
    use super::assess_h1_artifacts;
    use crate::h1_workflow_template;

    fn valid_binding() -> String {
        serde_json::json!({
            "admission": {
                "scope": {
                    "kind": "repository",
                    "owner": "fixture-owner",
                    "repository": "fixture-repository"
                },
                "runner_id": 1,
                "runner_name": "fixture-runner",
                "reserved_label": "runnermesh-admit",
                "credential_ref": {
                    "provider": "windows-credential-manager",
                    "key": "fixture-credential"
                },
                "ownership": {
                    "scope": {
                        "kind": "repository",
                        "owner": "fixture-owner",
                        "repository": "fixture-repository"
                    },
                    "runner_id": 1,
                    "label": "runnermesh-admit"
                }
            },
            "local": {
                "runner_home": "C:\\fixture\\runner",
                "work_root": "C:\\fixture\\work",
                "listener_image": "C:\\fixture\\runner\\bin\\Runner.Listener.exe",
                "worker_image": "C:\\fixture\\runner\\bin\\Runner.Worker.exe",
                "execution_identity_ref": {
                    "provider": "fixture-owner-envelope",
                    "key": "fixture-identity"
                }
            },
            "workflow": {
                "owner": "fixture-owner",
                "repository": "fixture-repository",
                "workflow_path": ".github/workflows/runnermesh-h1.yml",
                "immutable_ref": "1111111111111111111111111111111111111111",
                "expected_blob_sha": "2222222222222222222222222222222222222222",
                "expected_runner_name": "fixture-runner"
            },
            "restore": {
                "transaction_family": "h1-github-native-admission-label-v1",
                "baseline": {
                    "admission": "ADVERTISED",
                    "local_runner_expected_online": true
                },
                "recovery_plan_ref": "fixture-restore-v1"
            }
        })
        .to_string()
    }

    #[test]
    fn exact_binding_and_frozen_workflow_are_ready() {
        let assessment = assess_h1_artifacts(&valid_binding(), h1_workflow_template());
        assert!(assessment.ready());
    }

    #[test]
    fn placeholder_is_refused_even_when_other_semantics_are_valid() {
        let binding = valid_binding().replace("fixture-identity", "REPLACE_WITH_IDENTITY");
        let assessment = assess_h1_artifacts(&binding, h1_workflow_template());
        assert!(assessment.binding_schema_valid);
        assert!(!assessment.placeholders_absent);
        assert!(!assessment.binding_semantics_valid);
        assert!(!assessment.ready());
    }

    #[test]
    fn unknown_nested_binding_member_is_refused() {
        let binding = valid_binding().replace(
            "\"kind\":\"repository\"",
            "\"kind\":\"repository\",\"unexpected\":true",
        );
        let assessment = assess_h1_artifacts(&binding, h1_workflow_template());
        assert!(!assessment.binding_schema_valid);
        assert!(!assessment.ready());
    }

    #[test]
    fn exact_types_nonzero_identity_and_cross_bindings_are_required() {
        let mut wrong_type: serde_json::Value =
            serde_json::from_str(&valid_binding()).expect("fixture is JSON");
        wrong_type["admission"]["runner_id"] = serde_json::json!("1");
        let wrong_type = assess_h1_artifacts(&wrong_type.to_string(), h1_workflow_template());
        assert!(!wrong_type.binding_schema_valid);

        let mut zero_id: serde_json::Value =
            serde_json::from_str(&valid_binding()).expect("fixture is JSON");
        zero_id["admission"]["runner_id"] = serde_json::json!(0);
        let zero_id = assess_h1_artifacts(&zero_id.to_string(), h1_workflow_template());
        assert!(zero_id.binding_schema_valid);
        assert!(!zero_id.binding_semantics_valid);

        let mut ownership_drift: serde_json::Value =
            serde_json::from_str(&valid_binding()).expect("fixture is JSON");
        ownership_drift["admission"]["ownership"]["runner_id"] = serde_json::json!(2);
        let ownership_drift =
            assess_h1_artifacts(&ownership_drift.to_string(), h1_workflow_template());
        assert!(ownership_drift.binding_schema_valid);
        assert!(!ownership_drift.binding_semantics_valid);

        let mut workflow_drift: serde_json::Value =
            serde_json::from_str(&valid_binding()).expect("fixture is JSON");
        workflow_drift["workflow"]["repository"] = serde_json::json!("other-repository");
        let workflow_drift =
            assess_h1_artifacts(&workflow_drift.to_string(), h1_workflow_template());
        assert!(workflow_drift.binding_schema_valid);
        assert!(!workflow_drift.binding_semantics_valid);
    }

    #[test]
    fn workflow_drift_is_refused_independently_of_binding() {
        let workflow = h1_workflow_template().replace(
            "$env:H1_TRANSACTION_ID -cne $env:H1_EXPECTED_TRANSACTION_ID",
            "$env:H1_TRANSACTION_ID -cne $env:H1_TRANSACTION_ID",
        );
        let assessment = assess_h1_artifacts(&valid_binding(), &workflow);
        assert!(assessment.binding_semantics_valid);
        assert!(!assessment.workflow_source_contract_ready);
        assert!(!assessment.ready());
    }
}
