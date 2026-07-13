declare module '@earendil-works/pi-ai' {
  export const Type: {
    Object: (properties: Record<string, unknown>) => unknown
    String: (options?: Record<string, unknown>) => unknown
    Number: (options?: Record<string, unknown>) => unknown
    Optional: (schema: unknown) => unknown
  }
}
