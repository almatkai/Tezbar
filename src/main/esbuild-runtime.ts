export function configurePackagedEsbuildBinary(): void {
  // The Tauri host sets ESBUILD_BINARY_PATH to its packaged resource before
  // launching the backend sidecar. Development uses esbuild's normal lookup.
}
