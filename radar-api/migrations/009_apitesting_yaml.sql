-- Add api-testing YAML output alongside the existing Postman collection JSON.
ALTER TABLE generated_test_suite ADD COLUMN apitesting_yaml TEXT;
