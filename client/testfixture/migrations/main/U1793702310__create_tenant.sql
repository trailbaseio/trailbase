-- Create a new tenant.
INSERT INTO _tenant (id, name, template) VALUES (uuid_parse('f47ac10b-58cc-4372-a567-0e02b2c3d479'), 'test_tenant', 'test_template');

-- Add admin user to test tenant.
INSERT INTO _tenant_member (tenant, user) SELECT _tenant.id, _user.id FROM _tenant, _user
  WHERE _tenant.name = 'test_tenant' AND _user.email = 'admin@localhost';
