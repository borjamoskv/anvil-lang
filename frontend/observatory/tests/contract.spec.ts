import { test, expect } from '@playwright/test';

// C5-REAL: Strict API Contract Tests
test.describe('Contract_Lock: Ledger API', () => {
  // We need to run these against the backend Rust API running locally
  // Assuming it runs on http://localhost:3000
  const API_URL = 'http://localhost:3000';

  test('invalid_metric_id returns 404', async ({ request }) => {
    const response = await request.get(`${API_URL}/api/provenance/invalid-id-1234`);
    expect(response.status()).toBe(404);
  });

  // We can't easily test valid_metric_id here without seeding the DB first,
  // but we can ensure that ANY valid response returned matches the strict schema
  // and has data_origin correctly mapped.
});
