package app

import (
	"testing"

	"github.com/agent-team-project/kensho-projects/projects/workplane/internal/api/generated"
)

func TestRequiredEvidenceMetadataPresence(t *testing.T) {
	t.Parallel()
	content := "result"
	input := generated.EvidenceInput{
		Kind: "test-run", Title: "Exact result", Claim: "The gate passes", Source: "smoke", Content: &content,
		Supports: []generated.EvidenceSupportInput{{TargetType: "deliverable", TargetID: "00000000-0000-4000-8000-000000000001"}},
	}
	if _, ok := normalizeEvidenceInput(input); ok {
		t.Fatal("omitted required metadata was accepted")
	}
	input.Metadata = map[string]string{}
	if _, ok := normalizeEvidenceInput(input); !ok {
		t.Fatal("explicit empty metadata object was rejected")
	}
}

func TestRequiredVerdictFindingsPresence(t *testing.T) {
	t.Parallel()
	input := generated.VerdictInput{
		Result: "pass", EvidenceIDs: []string{"00000000-0000-4000-8000-000000000001"},
	}
	if _, ok := normalizeVerdictInput(input); ok {
		t.Fatal("omitted required findings was accepted")
	}
	input.Findings = []generated.FindingInput{}
	if _, ok := normalizeVerdictInput(input); !ok {
		t.Fatal("explicit empty findings array was rejected")
	}
}

func TestIndependentReviewerChecksEveryEvidenceSet(t *testing.T) {
	t.Parallel()
	actor := Actor{ID: "reviewer"}
	deliverable := Deliverable{CreatedBy: "creator"}
	submission := Submission{SubmittedBy: "submitter"}
	submitted := []Evidence{{ProducedBy: "submitted-producer"}}
	verdict := []Evidence{{ProducedBy: "verdict-producer"}}
	if !independentReviewer(actor, deliverable, submission, submitted, verdict) {
		t.Fatal("independent reviewer was rejected")
	}
	for _, identity := range []string{"creator", "submitter", "submitted-producer", "verdict-producer"} {
		actor.ID = identity
		if independentReviewer(actor, deliverable, submission, submitted, verdict) {
			t.Fatalf("reviewer identity %q bypassed separation of duty", identity)
		}
	}
}
