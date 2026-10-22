// Keep cancellation side-effect free; shared by the actual clear-history handler and tests.
export async function confirmedOperation(confirm, operation) {
  if (!await confirm()) return false;
  await operation();
  return true;
}
