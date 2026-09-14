-- Public provider market config 2_48: full-time total corners, verified
-- https://sportsbookv2.iddaa.com/sportsbook/get_market_config
-- Preserve prices/timestamps and original provider codes; repair only metadata.
CREATE VIEW iddaa_model_odds AS
SELECT o.*,
 CASE WHEN provider='iddaa' AND market_code='48' AND line_value IS NOT NULL
 THEN 'FULL_TIME_TOTAL_CORNERS'
 WHEN normalized_market_type='BOTH_TEAMS_TO_SCORE' THEN 'BTTS'
 WHEN normalized_market_type='CORNERS_TOTAL' THEN 'FULL_TIME_TOTAL_CORNERS'
 ELSE COALESCE(normalized_market_type,market_name) END AS model_market_type,
 CASE WHEN provider='iddaa' AND market_code='48' AND line_value IS NOT NULL
 THEN CASE WHEN selection IN ('Üst','Ust','üst','ust') THEN 'OVER'
 WHEN selection IN ('Alt','alt') THEN 'UNDER' ELSE normalized_selection END
 ELSE COALESCE(normalized_selection,selection) END AS model_selection
FROM odds_snapshots o;
