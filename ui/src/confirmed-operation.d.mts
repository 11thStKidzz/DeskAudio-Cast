export function confirmedOperation(confirm: () => Promise<boolean>, operation: () => Promise<void>): Promise<boolean>;
