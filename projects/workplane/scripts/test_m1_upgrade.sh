#!/bin/sh
set -eu

cd "$(dirname "$0")/.."
project="workplane_m1_upgrade_${$}"

cleanup() {
  docker compose --project-name "$project" down --volumes --remove-orphans >/dev/null 2>&1 || true
}
trap cleanup EXIT INT TERM

docker compose --project-name "$project" up -d --wait postgres
for migration in migrations/000001_m0_identity_fixture.up.sql migrations/000002_m1_walking_slice.up.sql; do
  docker compose --project-name "$project" exec -T postgres \
    psql -v ON_ERROR_STOP=1 -U workplane -d workplane < "$migration"
done
docker compose --project-name "$project" exec -T postgres \
  psql -v ON_ERROR_STOP=1 -U workplane -d workplane < fixtures/reference-organization.sql

docker compose --project-name "$project" exec -T postgres \
  psql -v ON_ERROR_STOP=1 -U workplane -d workplane <<'SQL'
BEGIN;

INSERT INTO projects
  (id,organization_id,title,outcome,mode,state,version,hypothesis,falsifier,
   decision_criteria,experiment_bound,created_by,created_at,updated_at)
VALUES
  ('00000000-0000-4000-8000-000000000100','00000000-0000-4000-8000-000000000001',
   'Committed before M2','Prove the upgrade boundary','exploration','proposed',2,
   'M1 events can become durably deliverable','Any missing outbox or delivery-state row',
   '["one event, one outbox, one state"]','One project and one decision',
   '00000000-0000-4000-8000-000000000010','2026-07-10T01:00:00Z','2026-07-10T01:01:00Z');

INSERT INTO decisions
  (id,organization_id,project_id,kind,question,choice,alternatives,rationale,
   evidence,consequences,actor_id,recorded_at)
VALUES
  ('00000000-0000-4000-8000-000000000110','00000000-0000-4000-8000-000000000001',
   '00000000-0000-4000-8000-000000000100','continue','Can M2 upgrade the M1 ledger?',
   'Apply migration 3','[]','The durable spine must cover already committed events',
   '["M1 project event","M1 decision event"]','["No event is stranded"]',
   '00000000-0000-4000-8000-000000000012','2026-07-10T01:01:00Z');

INSERT INTO domain_events
  (event_id,organization_id,aggregate_type,aggregate_id,aggregate_version,event_type,
   schema_version,actor_kind,actor_id,principal_id,command_id,request_id,occurred_at,payload)
VALUES
  ('00000000-0000-4000-8000-000000000120','00000000-0000-4000-8000-000000000001',
   'project','00000000-0000-4000-8000-000000000100',1,'project.created',1,
   'human','00000000-0000-4000-8000-000000000010',NULL,
   '00000000-0000-4000-8000-000000000130','m1-upgrade-project','2026-07-10T01:00:00Z',
   jsonb_build_object(
     'id','00000000-0000-4000-8000-000000000100',
     'organization_id','00000000-0000-4000-8000-000000000001',
     'title','Committed before M2','outcome','Prove the upgrade boundary',
     'mode','exploration','state','proposed','version',1,
     'hypothesis','M1 events can become durably deliverable',
     'falsifier','Any missing outbox or delivery-state row',
     'decision_criteria',jsonb_build_array('one event, one outbox, one state'),
     'experiment_bound','One project and one decision')),
  ('00000000-0000-4000-8000-000000000121','00000000-0000-4000-8000-000000000001',
   'project','00000000-0000-4000-8000-000000000100',2,'decision.recorded',1,
   'agent','00000000-0000-4000-8000-000000000012','00000000-0000-4000-8000-000000000010',
   '00000000-0000-4000-8000-000000000131','m1-upgrade-decision','2026-07-10T01:01:00Z',
   jsonb_build_object(
     'id','00000000-0000-4000-8000-000000000110',
     'project_id','00000000-0000-4000-8000-000000000100',
     'actor_id','00000000-0000-4000-8000-000000000012','actor_kind','agent',
     'principal_id','00000000-0000-4000-8000-000000000010',
     'recorded_at','2026-07-10T01:01:00.000000Z','kind','continue',
     'question','Can M2 upgrade the M1 ledger?','choice','Apply migration 3',
     'alternatives',jsonb_build_array(),
     'rationale','The durable spine must cover already committed events',
     'evidence',jsonb_build_array('M1 project event','M1 decision event'),
     'consequences',jsonb_build_array('No event is stranded')));

COMMIT;
SQL

pre_upgrade="$(docker compose --project-name "$project" exec -T postgres \
  psql -At -U workplane -d workplane -c \
  "SELECT max(version) || '|' || (SELECT count(*) FROM projects) || '|' ||
    (SELECT count(*) FROM decisions) || '|' || (SELECT count(*) FROM domain_events) || '|' ||
    COALESCE(to_regclass('public.outbox_records')::text,'absent') FROM schema_migrations;")"
if [ "$pre_upgrade" != "2|1|1|2|absent" ]; then
  echo "unexpected committed M1 database before upgrade: $pre_upgrade" >&2
  exit 1
fi

docker compose --project-name "$project" exec -T postgres \
  psql -v ON_ERROR_STOP=1 -U workplane -d workplane < migrations/000003_m2_durable_spine.up.sql

coverage="$(docker compose --project-name "$project" exec -T postgres \
  psql -At -U workplane -d workplane -c \
  "SELECT max(version) || '|' || (SELECT count(*) FROM projects) || '|' ||
    (SELECT count(*) FROM decisions) || '|' || (SELECT count(*) FROM domain_events) || '|' ||
    (SELECT count(*) FROM outbox_records) || '|' ||
    (SELECT count(*) FROM outbox_delivery_state WHERE consumer_name='projection-v1') || '|' ||
    checkpoint.last_sequence || '|' || COALESCE(checkpoint.last_event_id::text,'none')
   FROM schema_migrations CROSS JOIN consumer_checkpoints AS checkpoint
   WHERE checkpoint.consumer_name='projection-v1' GROUP BY checkpoint.last_sequence,checkpoint.last_event_id;")"
if [ "$coverage" != "3|1|1|2|2|2|0|none" ]; then
  echo "M1 upgrade did not create one outbox and state row per event: $coverage" >&2
  exit 1
fi

mismatches="$(docker compose --project-name "$project" exec -T postgres \
  psql -At -U workplane -d workplane -c \
  "SELECT count(*)
   FROM domain_events AS event
   LEFT JOIN outbox_records AS record
     ON record.event_sequence=event.sequence AND record.event_id=event.event_id
   LEFT JOIN outbox_delivery_state AS state
     ON state.consumer_name='projection-v1' AND state.event_id=event.event_id
   WHERE record.event_id IS NULL OR state.event_id IS NULL
     OR record.id<>event.event_id OR record.topic<>'domain-events'
     OR record.payload IS DISTINCT FROM jsonb_build_object(
       'sequence',event.sequence,'event_id',event.event_id,'event_type',event.event_type,
       'schema_version',event.schema_version,'organization_id',event.organization_id,
       'aggregate_type',event.aggregate_type,'aggregate_id',event.aggregate_id,
       'aggregate_version',event.aggregate_version,'command_id',event.command_id,
       'request_id',event.request_id,'actor_kind',event.actor_kind,'actor_id',event.actor_id,
       'principal_id',event.principal_id,
       'occurred_at',to_char(event.occurred_at AT TIME ZONE 'UTC','YYYY-MM-DD\"T\"HH24:MI:SS.US\"Z\"'),
       'payload',event.payload)
     OR record.payload_sha256 IS DISTINCT FROM digest(convert_to(record.payload::text,'UTF8'),'sha256')
     OR record.available_at IS DISTINCT FROM event.occurred_at
     OR record.created_at IS DISTINCT FROM event.occurred_at
     OR state.attempts<>0 OR state.lease_owner IS NOT NULL OR state.lease_until IS NOT NULL
     OR state.delivered_at IS NOT NULL;")"
if [ "$mismatches" != "0" ]; then
  echo "M1 upgrade produced non-canonical outbox or delivery-state rows: $mismatches" >&2
  exit 1
fi

event_state="$(docker compose --project-name "$project" exec -T postgres \
  psql -At -U workplane -d workplane -c \
  "SELECT string_agg(event.event_type || ':' || event.aggregate_version || ':' || state.attempts,
     ',' ORDER BY event.sequence)
   FROM domain_events AS event
   JOIN outbox_records AS record ON record.event_sequence=event.sequence AND record.event_id=event.event_id
   JOIN outbox_delivery_state AS state
     ON state.consumer_name='projection-v1' AND state.event_id=record.event_id;")"
if [ "$event_state" != "project.created:1:0,decision.recorded:2:0" ]; then
  echo "unexpected upgraded event delivery state: $event_state" >&2
  exit 1
fi

printf '%s\n' "M1-to-M2 PostgreSQL upgrade passed: coverage=$coverage events=$event_state"

docker compose --project-name "$project" exec -T postgres \
  psql -v ON_ERROR_STOP=1 -U workplane -d workplane < migrations/000004_m2_realtime_spine.up.sql

realtime_coverage="$(docker compose --project-name "$project" exec -T postgres \
  psql -At -U workplane -d workplane -c \
  "SELECT max(version) || '|' || (SELECT count(*) FROM outbox_records) || '|' ||
    (SELECT count(*) FROM outbox_delivery_state WHERE consumer_name='realtime-v1') || '|' ||
    (SELECT last_sequence FROM consumer_checkpoints WHERE consumer_name='realtime-v1') || '|' ||
    (SELECT count(*) FROM project_memberships) || '|' || (SELECT count(*) FROM realtime_retention)
   FROM schema_migrations;")"
if [ "$realtime_coverage" != "4|2|2|0|1|1" ]; then
  echo "M2B upgrade did not backfill realtime consumer/authority state: $realtime_coverage" >&2
  exit 1
fi

printf '%s\n' "M2A-to-M2B PostgreSQL upgrade passed: coverage=$realtime_coverage"

docker compose --project-name "$project" exec -T postgres \
  psql -v ON_ERROR_STOP=1 -U workplane -d workplane < migrations/000005_m2_realtime_checkpoint_horizon.up.sql

commit_horizon="$(docker compose --project-name "$project" exec -T postgres \
  psql -At -U workplane -d workplane -c \
  "SELECT max(version) || '|' || to_regprocedure('lock_domain_event_commit_horizon()')::text || '|' ||
    has_function_privilege('workplane_app','lock_domain_event_commit_horizon()','EXECUTE')
   FROM schema_migrations;")"
if [ "$commit_horizon" != "5|lock_domain_event_commit_horizon()|true" ]; then
  echo "M2B checkpoint-horizon upgrade failed: $commit_horizon" >&2
  exit 1
fi

printf '%s\n' "M2B checkpoint-horizon upgrade passed: shape=$commit_horizon"

docker compose --project-name "$project" exec -T postgres \
  psql -v ON_ERROR_STOP=1 -U workplane -d workplane < migrations/000006_m2_planning_contracts.up.sql

planning_upgrade="$(docker compose --project-name "$project" exec -T postgres \
  psql -At -U workplane -d workplane -c \
  "SELECT max(version) || '|' || (SELECT count(*) FROM projects) || '|' ||
    (SELECT count(*) FROM decisions) || '|' || (SELECT count(*) FROM domain_events) || '|' ||
    (SELECT count(*) FROM outbox_records) || '|' || (SELECT count(*) FROM deliverables) || '|' ||
    (SELECT count(*) FROM forecasts) || '|' ||
    COALESCE((SELECT planning_count FROM projection_heads WHERE name='m1-canonical')::text,'no-head')
   FROM schema_migrations;")"
if [ "$planning_upgrade" != "6|1|1|2|2|0|0|no-head" ]; then
  echo "M2B-to-M2C upgrade changed accepted rows or failed to add planning shape: $planning_upgrade" >&2
  exit 1
fi

printf '%s\n' "M2B-to-M2C PostgreSQL upgrade passed without rewriting accepted ledger rows: shape=$planning_upgrade"
