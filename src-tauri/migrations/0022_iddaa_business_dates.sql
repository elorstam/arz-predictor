-- Turkey has used UTC+03:00 throughout the modern Iddaa bulletin period.
-- Preserve an audit of each corrected derived date; fixture/prediction identity stays intact.
INSERT INTO resolution_audit_log(entity_type,source_entity,target_entity,action,method,confidence,details_json)
SELECT 'MATCH',CAST(m.id AS TEXT),date(m.kickoff_at,'+3 hours'),'BUSINESS_DATE_CORRECTION','MIGRATION_0022',1,
 json_object('old_date',m.scheduled_local_date,'new_date',date(m.kickoff_at,'+3 hours'),'timezone','Europe/Istanbul')
FROM matches m WHERE m.kickoff_at>='2016-09-07' AND m.scheduled_local_date<>date(m.kickoff_at,'+3 hours')
 AND EXISTS(SELECT 1 FROM provider_match_mappings p WHERE p.match_id=m.id AND p.provider='iddaa');
UPDATE matches SET scheduled_local_date=date(kickoff_at,'+3 hours')
WHERE kickoff_at>='2016-09-07' AND scheduled_local_date<>date(kickoff_at,'+3 hours')
 AND EXISTS(SELECT 1 FROM provider_match_mappings p WHERE p.match_id=matches.id AND p.provider='iddaa');
