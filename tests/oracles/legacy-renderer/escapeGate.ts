/** Lets the command surface intercept Escape before App hides the window.
 *  Registered while `CommandBar` is mounted; see `tryConsumeCommandSurfaceEscape`. */
let commandSurfaceEscapeConsumer: (() => boolean) | null = null

export function setCommandSurfaceEscapeConsumer(fn: (() => boolean) | null): void {
  commandSurfaceEscapeConsumer = fn
}

/** @returns true if Escape was handled (caller must not hide the window). */
export function tryConsumeCommandSurfaceEscape(): boolean {
  return commandSurfaceEscapeConsumer?.() ?? false
}
