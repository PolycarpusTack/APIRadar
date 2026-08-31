-- Down-migration for 009_apitesting_yaml.up.sql
--

ALTER TABLE generated_test_suite DROP COLUMN apitesting_yaml;
