import {
  app,
  BrowserWindow,
  dialog,
  globalShortcut,
  ipcMain,
  Menu,
  type MenuItemConstructorOptions,
  nativeImage,
  screen,
  session,
  Tray,
} from 'electron'
import { join } from 'node:path'
import { existsSync } from 'node:fs'

function isStdioWriteEio(error: unknown): boolean {
  return (
    error instanceof Error &&
    (error as NodeJS.ErrnoException).code === 'EIO' &&
    (error as NodeJS.ErrnoException).syscall === 'write'
  )
}

process.stdout?.on('error', () => {})
process.stderr?.on('error', () => {})
process.on('uncaughtException', (error) => {
  if (isStdioWriteEio(error)) return
  throw error
})
import {
  DEFAULT_RAYMES_HOTKEY,
  flushConfig,
  getRaymesHotkey,
  getUiStateRetentionMs,
  setRaymesHotkey,
} from './llm/configStore'
import { registerIpcHandlers, shutdownIpcHandlers } from './ipc'
import { startClipboardWatcher, stopClipboardWatcher } from './search/providers/clipboardProvider'
import {
  WINDOW_MAX_HEIGHT,
  WINDOW_MIN_HEIGHT,
  WINDOW_TOP_FACTOR,
  WINDOW_WIDTH,
} from './windowBounds'

import { isPhysicalKeyDown } from './bridge'
import { shouldSuppressBlurHide } from './windowState'
import { getPersistedWindowPosition, setPersistedWindowPosition } from './llm/configStore'
import {
  cleanupCenterOverlay,
  hideCenterOverlay,
  prepareCenterOverlay,
  showCenterOverlay,
} from './center-overlay'

let mainWindow: BrowserWindow | null = null
let settingsWindow: BrowserWindow | null = null
let tray: Tray | null = null
let commandBarVisible = false
let isAppQuitting = false
let raymesHotkey = getRaymesHotkey()
type AppSurface = 'command' | 'settings' | 'clipboard'
/** Set when the palette is hidden; used to decide whether to reset renderer UI on reopen. */
let lastPaletteHideAt: number | null = null

/* ---------------------------------------------------------------------------
   Snap-to-center constants
   --------------------------------------------------------------------------- */
/** Distance (px) at which the window magnetically snaps to screen center. */
const SNAP_THRESHOLD = 12
const UNSNAP_BUFFER = 6
let dragMonitorTimer: NodeJS.Timeout | null = null
let dragReleaseTimer: NodeJS.Timeout | null = null
let dragFinalizeTimer: NodeJS.Timeout | null = null
let dragSessionActive = false
let dragSnapLocked = false
let isMouseDown = false

let lastSnapPayload: { visible: boolean; active: boolean } | null = null

function sendWindowSnapGuides(
  win: BrowserWindow,
  payload: { visible: boolean; active: boolean }
): void {
  if (win.isDestroyed() || win.webContents.isDestroyed()) return

  // Only send and log if the state has actually changed
  if (
    lastSnapPayload &&
    lastSnapPayload.visible === payload.visible &&
    lastSnapPayload.active === payload.active
  ) {
    return
  }

  lastSnapPayload = { ...payload }
  win.webContents.send('window:snap-guides', payload)
}

/** Return the top-left position that would center `win` on its nearest display. */
function getScreenCenter(win: BrowserWindow): { x: number; y: number } {
  const bounds = win.getBounds()
  const display = screen.getDisplayNearestPoint({
    x: bounds.x + Math.floor(bounds.width / 2),
    y: bounds.y + Math.floor(bounds.height / 2),
  })
  const { workArea } = display
  return {
    x: workArea.x + Math.floor((workArea.width - bounds.width) / 2),
    y: workArea.y + Math.floor((workArea.height - bounds.height) / 2),
  }
}

function snapWindowToCenter(win: BrowserWindow): void {
  if (win.isDestroyed()) return
  const bounds = win.getBounds()
  const center = getScreenCenter(win)
  if (bounds.x === center.x && bounds.y === center.y) return
  isProgrammaticMove = true
  win.setBounds(
    {
      x: center.x,
      y: center.y,
      width: bounds.width,
      height: bounds.height,
    },
    false
  )
  setTimeout(() => {
    isProgrammaticMove = false
  }, 0)
}

function updateWindowSnapState(win: BrowserWindow): void {
  if (win.isDestroyed() || isProgrammaticMove) return
  const bounds = win.getBounds()
  const display = screen.getDisplayNearestPoint({
    x: bounds.x + Math.floor(bounds.width / 2),
    y: bounds.y + Math.floor(bounds.height / 2),
  })
  const { workArea } = display
  const windowCenterX = bounds.x + bounds.width / 2
  const windowCenterY = bounds.y + bounds.height / 2
  const screenCenterX = workArea.x + workArea.width / 2
  const screenCenterY = workArea.y + workArea.height / 2
  const dx = Math.abs(windowCenterX - screenCenterX)
  const dy = Math.abs(windowCenterY - screenCenterY)

  const releaseThreshold = SNAP_THRESHOLD + UNSNAP_BUFFER

  if (dragSnapLocked) {
    if (dx > releaseThreshold || dy > releaseThreshold) {
      dragSnapLocked = false
      sendWindowSnapGuides(win, { visible: true, active: false })
      showCenterOverlay(win, 'approaching')
      return
    }
    snapWindowToCenter(win)
    sendWindowSnapGuides(win, { visible: true, active: true })
    showCenterOverlay(win, 'snap-ready')
    return
  }

  const nearVerticalCenter = dx < SNAP_THRESHOLD
  const nearHorizontalCenter = dy < SNAP_THRESHOLD
  if (nearVerticalCenter && nearHorizontalCenter) {
    dragSnapLocked = true
    snapWindowToCenter(win)
    sendWindowSnapGuides(win, { visible: true, active: true })
    showCenterOverlay(win, 'snap-ready')
    return
  }
  sendWindowSnapGuides(win, { visible: true, active: false })
  showCenterOverlay(win, 'approaching')
}

function startWindowDragMonitoring(win: BrowserWindow): void {
  if (dragFinalizeTimer !== null) {
    clearTimeout(dragFinalizeTimer)
    dragFinalizeTimer = null
  }
  if (win.isDestroyed() || dragSessionActive) return
  dragSessionActive = true
  dragSnapLocked = false
  sendWindowSnapGuides(win, { visible: true, active: false })
  showCenterOverlay(win, 'approaching')
  if (dragMonitorTimer !== null) {
    clearInterval(dragMonitorTimer)
  }
  dragMonitorTimer = setInterval(() => {
    updateWindowSnapState(win)
  }, 16)
}

function pauseWindowDragMonitoring(win: BrowserWindow): void {
  if (!dragSessionActive) return
  dragSessionActive = false
  dragSnapLocked = false
  if (dragMonitorTimer !== null) {
    clearInterval(dragMonitorTimer)
    dragMonitorTimer = null
  }
  if (dragReleaseTimer !== null) {
    clearTimeout(dragReleaseTimer)
    dragReleaseTimer = null
  }
  sendWindowSnapGuides(win, { visible: false, active: false })
  hideCenterOverlay()
}

function scheduleWindowDragFinalize(win: BrowserWindow): void {
  if (dragFinalizeTimer !== null) {
    clearTimeout(dragFinalizeTimer)
  }
  dragFinalizeTimer = setTimeout(() => {
    dragFinalizeTimer = null
    if (dragSessionActive || win.isDestroyed() || !isMouseDown) return
    const [curX, curY] = win.getPosition()
    setPersistedWindowPosition({ x: curX, y: curY })
  }, 900)
}

function scheduleWindowDragRelease(win: BrowserWindow): void {
  if (dragReleaseTimer !== null) {
    clearTimeout(dragReleaseTimer)
  }
  dragReleaseTimer = setTimeout(() => {
    dragReleaseTimer = null
    if (!dragSessionActive || win.isDestroyed()) return
    pauseWindowDragMonitoring(win)
    scheduleWindowDragFinalize(win)
  }, 120)
}

function stopWindowDragMonitoring(win: BrowserWindow): void {
  dragSessionActive = false
  dragSnapLocked = false
  isMouseDown = false
  if (dragFinalizeTimer !== null) {
    clearTimeout(dragFinalizeTimer)
    dragFinalizeTimer = null
  }
  if (dragReleaseTimer !== null) {
    clearTimeout(dragReleaseTimer)
    dragReleaseTimer = null
  }
  if (dragMonitorTimer !== null) {
    clearInterval(dragMonitorTimer)
    dragMonitorTimer = null
  }
  sendWindowSnapGuides(win, { visible: false, active: false })
  hideCenterOverlay()
  if (!win.isDestroyed()) {
    const [curX, curY] = win.getPosition()
    setPersistedWindowPosition({ x: curX, y: curY })
  }
}

function handleNativeWillMove(win: BrowserWindow): void {
  if (win.isDestroyed() || isProgrammaticMove) return

  startWindowDragMonitoring(win)
  scheduleWindowDragRelease(win)
}

function handleNativeMove(win: BrowserWindow): void {
  if (win.isDestroyed() || isProgrammaticMove) return

  // Keep drag session alive even when OS emits sparse move events.
  startWindowDragMonitoring(win)
  scheduleWindowDragRelease(win)
}

/** After Alt+Space opens the launcher, poll HID key state so a sustained chord
 *  starts local dictation (push-to-talk) while `globalShortcut` only fires once. */
const ALT_SPACE_HOLD_MS = 100
const ALT_SPACE_POLL_MS = 12
const ALT_SPACE_WATCH_MAX_MS = 120_000

let altSpaceHoldTimer: ReturnType<typeof setInterval> | null = null
let altSpaceHoldOpenedAt = 0
let altSpaceHotkeyDictationArmed = false
let altSpaceHoldTriggered = false
let altSpaceFocusedHoldTimer: ReturnType<typeof setTimeout> | null = null
let altSpaceFocusedPressedAt = 0
let altSpaceFocusedHoldTriggered = false

function releaseAltSpaceHotkeyDictation(): void {
  if (altSpaceHotkeyDictationArmed && mainWindow && !mainWindow.isDestroyed()) {
    mainWindow.webContents.send('voice:hotkey-hold', { phase: 'release' })
  }
  altSpaceHotkeyDictationArmed = false
}

function stopAltSpaceHoldWatcher(): void {
  if (altSpaceHoldTimer !== null) {
    clearInterval(altSpaceHoldTimer)
    altSpaceHoldTimer = null
  }
  releaseAltSpaceHotkeyDictation()
  altSpaceHoldTriggered = false
}

function stopFocusedAltSpaceGesture(): void {
  if (altSpaceFocusedHoldTimer !== null) {
    clearTimeout(altSpaceFocusedHoldTimer)
    altSpaceFocusedHoldTimer = null
  }
  altSpaceFocusedPressedAt = 0
  altSpaceFocusedHoldTriggered = false
}

function toggleCommandBarImmediate(): void {
  if (isAppQuitting) return
  if (!mainWindow) return

  if (commandBarVisible) {
    // If the palette is marked visible but not focused (e.g. user switched Space),
    // reopen it in the current active Space instead of toggling it off there.
    if (!mainWindow.isFocused()) {
      hideCommandBar()
      showCommandBar()
      return
    }
    hideCommandBar()
    return
  }

  showCommandBar()
}

function activateMicFromAltSpaceHold(): void {
  if (isAppQuitting) return
  if (!mainWindow || mainWindow.isDestroyed()) return

  // Hold gesture semantics:
  // - if closed: open + start dictation
  // - if open: keep it open and start dictation again
  if (!commandBarVisible || !mainWindow.isFocused()) {
    showCommandBar()
  }

  if (!altSpaceHotkeyDictationArmed) {
    mainWindow.webContents.send('voice:hotkey-hold', { phase: 'press' })
    altSpaceHotkeyDictationArmed = true
  }
  altSpaceHoldTriggered = true
}

function startFocusedAltSpaceGesture(): void {
  if (altSpaceFocusedPressedAt !== 0) return

  altSpaceFocusedPressedAt = Date.now()
  altSpaceFocusedHoldTriggered = false
  altSpaceFocusedHoldTimer = setTimeout(() => {
    altSpaceFocusedHoldTimer = null
    if (altSpaceFocusedPressedAt === 0) return
    activateMicFromAltSpaceHold()
    altSpaceFocusedHoldTriggered = true
  }, ALT_SPACE_HOLD_MS)
}

function finishFocusedAltSpaceGestureOnRelease(): void {
  if (altSpaceFocusedPressedAt === 0) return

  const elapsed = Date.now() - altSpaceFocusedPressedAt
  const holdTriggered = altSpaceFocusedHoldTriggered
  stopFocusedAltSpaceGesture()

  if (holdTriggered || elapsed >= ALT_SPACE_HOLD_MS) {
    if (!holdTriggered) {
      // Edge case: threshold elapsed, but timeout callback did not run yet.
      activateMicFromAltSpaceHold()
    }
    releaseAltSpaceHotkeyDictation()
    return
  }

  toggleCommandBarImmediate()
}

function isAltSpaceReleaseInput(input: Electron.Input): boolean {
  if (input.type !== 'keyUp') return false

  const key = input.key.toLowerCase()
  const code = input.code?.toLowerCase() ?? ''
  return (
    key === 'space' ||
    key === ' ' ||
    key === 'alt' ||
    key === 'option' ||
    code === 'space' ||
    code === 'altleft' ||
    code === 'altright'
  )
}

function isMouseReleaseInput(input: Electron.Input): boolean {
  const isUp = input.type === 'mouseUp'
  return isUp
}

function startAltSpaceHoldWatcher(): void {
  if (isAppQuitting) return
  stopAltSpaceHoldWatcher()
  if (process.platform !== 'darwin') {
    toggleCommandBarImmediate()
    return
  }

  altSpaceHoldOpenedAt = Date.now()
  altSpaceHotkeyDictationArmed = false
  altSpaceHoldTriggered = false
  showCommandBar()

  altSpaceHoldTimer = setInterval(() => {
    if (!mainWindow || mainWindow.isDestroyed()) {
      stopAltSpaceHoldWatcher()
      return
    }

    const elapsed = Date.now() - altSpaceHoldOpenedAt
    if (elapsed > ALT_SPACE_WATCH_MAX_MS) {
      stopAltSpaceHoldWatcher()
      return
    }

    let combo = false
    try {
      combo = isPhysicalKeyDown('space') && isPhysicalKeyDown('option')
    } catch {
      stopAltSpaceHoldWatcher()
      return
    }

    if (altSpaceHoldTriggered) {
      if (!combo) {
        stopAltSpaceHoldWatcher()
      }
      return
    }

    // Only classify tap-vs-hold once we cross the threshold so a key-down
    // never toggles immediately.
    if (elapsed < ALT_SPACE_HOLD_MS) {
      return
    }

    // Still held at threshold => hold behavior (activate mic).
    if (combo) {
      activateMicFromAltSpaceHold()
      return
    }

    // The launcher was already presented on key-down. A quick tap only
    // finishes gesture detection; a sustained chord activates dictation.
    stopAltSpaceHoldWatcher()
  }, ALT_SPACE_POLL_MS)
}

function handleAltSpaceHotkey(): void {
  if (isAppQuitting) return
  if (!mainWindow || mainWindow.isDestroyed()) return

  if (commandBarVisible && mainWindow.isFocused()) {
    startFocusedAltSpaceGesture()
    return
  }

  stopFocusedAltSpaceGesture()
  startAltSpaceHoldWatcher()
}

function createTrayIcon(): Electron.NativeImage {
  const possiblePaths = [
    join(process.resourcesPath, 'trayIconTemplate.png'),
    join(app.getAppPath(), 'resources', 'trayIconTemplate.png'),
  ]
  for (const p of possiblePaths) {
    if (existsSync(p)) {
      try {
        const img = nativeImage.createFromPath(p)
        if (!img.isEmpty()) {
          img.setTemplateImage(true)
          return img
        }
      } catch (e) {
        console.error(`Failed to load tray icon from ${p}:`, e)
      }
    }
  }
  return nativeImage.createFromDataURL(
    'data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAABAAAAAQCAAAAAAf8+9hAAAADElEQVR42mNgwMAAABgABXlY2Z8AAAAASUVORK5CYII='
  )
}

function isPaletteUiStale(lastHideAt: number | null, ttlMs: number): boolean {
  if (lastHideAt === null) return false
  if (ttlMs === 0) return true
  return Date.now() - lastHideAt > ttlMs
}

let isProgrammaticMove = false

function placeWindow(win: BrowserWindow): void {
  isProgrammaticMove = true
  const cursor = screen.getCursorScreenPoint()
  const activeDisplay = screen.getDisplayNearestPoint(cursor)
  const persisted = getPersistedWindowPosition()
  if (persisted) {
    const displays = screen.getAllDisplays()
    const persistedDisplay = displays.find((display) => {
      const bounds = display.bounds
      return (
        persisted.x >= bounds.x &&
        persisted.x < bounds.x + bounds.width &&
        persisted.y >= bounds.y &&
        persisted.y < bounds.y + bounds.height
      )
    })
    if (persistedDisplay?.id === activeDisplay.id) {
      win.setPosition(persisted.x, persisted.y)
      setTimeout(() => {
        isProgrammaticMove = false
      }, 100)
      return
    }
  }

  const { width, height, x, y } = activeDisplay.workArea
  const [, curH] = win.getContentSize()
  const contentH = Math.max(WINDOW_MIN_HEIGHT, curH || WINDOW_MAX_HEIGHT)
  const winX = x + Math.floor((width - WINDOW_WIDTH) / 2)
  const winY = y + Math.floor(height * WINDOW_TOP_FACTOR)
  win.setBounds({ x: winX, y: winY, width: WINDOW_WIDTH, height: contentH })
  setTimeout(() => {
    isProgrammaticMove = false
  }, 100)
}

function showCommandBar(): void {
  if (isAppQuitting) return
  if (!mainWindow) return

  // macOS keeps a window attached to its last Space; temporarily showing on all
  // workspaces lets us present it in the currently active Space, then we disable
  // that mode right away so it does not stay visible everywhere.
  if (process.platform === 'darwin') {
    mainWindow.setVisibleOnAllWorkspaces(true, {
      visibleOnFullScreen: true,
      skipTransformProcessType: true,
    })
  }

  placeWindow(mainWindow)
  prepareCenterOverlay(mainWindow)
  mainWindow.show()
  mainWindow.focus()
  commandBarVisible = true

  if (process.platform === 'darwin') {
    setTimeout(() => {
      if (!mainWindow || mainWindow.isDestroyed()) return
      mainWindow.setVisibleOnAllWorkspaces(false, {
        visibleOnFullScreen: true,
      })
    }, 0)
  }
}

function openAppSurface(surface: AppSurface): void {
  if (isAppQuitting) return
  if (surface === 'settings') {
    openSettingsWindow()
    return
  }
  showCommandBar()
  if (!mainWindow || mainWindow.isDestroyed()) return
  mainWindow.webContents.send('app:open-surface', surface)
}

function hideCommandBar(): void {
  stopFocusedAltSpaceGesture()
  stopAltSpaceHoldWatcher()
  if (!mainWindow) return

  commandBarVisible = false
  lastPaletteHideAt = Date.now()
  stopWindowDragMonitoring(mainWindow)
  mainWindow.hide()
}

function createWindow(): void {
  mainWindow = new BrowserWindow({
    width: WINDOW_WIDTH,
    height: WINDOW_MAX_HEIGHT,
    minWidth: WINDOW_WIDTH,
    maxWidth: WINDOW_WIDTH,
    minHeight: WINDOW_MIN_HEIGHT,
    maxHeight: WINDOW_MAX_HEIGHT,
    show: false,
    frame: false,
    resizable: false,
    minimizable: false,
    maximizable: false,
    fullscreenable: false,
    alwaysOnTop: true,
    skipTaskbar: true,
    transparent: true,
    backgroundColor: '#00000000',
    hasShadow: false,
    roundedCorners: false,
    webPreferences: {
      preload: join(__dirname, '../preload/index.js'),
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: true,
    },
  })

  if (process.platform === 'darwin') {
    mainWindow.setAlwaysOnTop(true, 'floating')
  }

  // Native drag lifecycle for frameless windows is most reliable on macOS.
  // This avoids depending on renderer mouse events in `-webkit-app-region: drag`.
  mainWindow.on('will-move', () => {
    // will-move fires when the OS initiates a drag, so this is our best indicator
    // that a drag session is starting (even before mouseDown reaches main process)
    isMouseDown = true
    if (mainWindow) handleNativeWillMove(mainWindow)
  })
  mainWindow.on('move', () => {
    if (mainWindow && dragSessionActive) {
      scheduleWindowDragRelease(mainWindow)
    }
    if (mainWindow && isMouseDown) handleNativeMove(mainWindow)
  })

  mainWindow.on('moved', () => {
    if (dragSessionActive && mainWindow) {
      scheduleWindowDragRelease(mainWindow)
    }
  })

  mainWindow.on('blur', () => {
    if (shouldSuppressBlurHide()) return
    stopWindowDragMonitoring(mainWindow!)
    hideCommandBar()
  })

  mainWindow.on('show', () => {
    if (!mainWindow || mainWindow.isDestroyed()) return
    const ttl = getUiStateRetentionMs()
    const stale = isPaletteUiStale(lastPaletteHideAt, ttl)
    mainWindow.webContents.send('window-shown', { resetUi: stale })
  })

  mainWindow.webContents.on('before-input-event', (_event, input) => {
    if (mainWindow && isMouseReleaseInput(input)) {
      // Reliable release hook for frameless drag regions: finalize drag state,
      // persist last coordinates, and hide overlay immediately on mouse-up.
      stopWindowDragMonitoring(mainWindow)
    }
    if (altSpaceFocusedPressedAt === 0) return
    if (!isAltSpaceReleaseInput(input)) return
    finishFocusedAltSpaceGestureOnRelease()
  })

  // Reliable release hook for frameless drag regions: finalize drag state,
  // persist last coordinates, and hide overlay immediately on mouse-up.
  mainWindow.webContents.on('cursor-changed', (_event, type) => {
    if (type === 'default' && dragSessionActive && mainWindow) {
      stopWindowDragMonitoring(mainWindow)
    }
  })

  loadRenderer(mainWindow)
}

function loadRenderer(win: BrowserWindow, search = ''): void {
  if (process.env['ELECTRON_RENDERER_URL']) {
    const url = new URL(process.env['ELECTRON_RENDERER_URL'])
    if (search) url.search = search
    void win.loadURL(url.toString())
    return
  }

  void win.loadFile(join(__dirname, '../renderer/index.html'), search ? { search } : undefined)
}

function openSettingsWindow(): void {
  if (isAppQuitting) return

  if (settingsWindow && !settingsWindow.isDestroyed()) {
    const existingWindow = settingsWindow
    existingWindow.hide()
    existingWindow.webContents.once('did-finish-load', () => {
      if (existingWindow.isDestroyed()) return
      existingWindow.show()
      existingWindow.focus()
    })
    loadRenderer(existingWindow, '?window=settings')
    return
  }

  settingsWindow = new BrowserWindow({
    width: 920,
    height: 680,
    minWidth: 760,
    minHeight: 520,
    title: 'TezBar Settings',
    show: false,
    frame: true,
    resizable: true,
    minimizable: true,
    maximizable: true,
    fullscreenable: true,
    alwaysOnTop: false,
    skipTaskbar: false,
    backgroundColor: '#1e1f2e',
    webPreferences: {
      preload: join(__dirname, '../preload/index.js'),
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: true,
    },
  })

  settingsWindow.on('closed', () => {
    settingsWindow = null
  })
  settingsWindow.on('page-title-updated', (event) => {
    event.preventDefault()
    settingsWindow?.setTitle('TezBar Settings')
  })
  settingsWindow.once('ready-to-show', () => {
    if (!settingsWindow || settingsWindow.isDestroyed()) return
    settingsWindow.show()
    settingsWindow.focus()
  })

  loadRenderer(settingsWindow, '?window=settings')
}

async function confirmAndQuitRaymes(): Promise<void> {
  if (isAppQuitting) return
  const options = {
    type: 'question' as const,
    buttons: ['Cancel', 'Quit'],
    defaultId: 1,
    cancelId: 0,
    title: 'Quit TezBar',
    message: 'Quit TezBar?',
    detail: 'Are you sure you want to quit TezBar and terminate all background processes?',
    noLink: true,
  }
  const result =
    mainWindow && !mainWindow.isDestroyed()
      ? await dialog.showMessageBox(mainWindow, options)
      : await dialog.showMessageBox(options)
  if (result.response !== 1) return

  isAppQuitting = true
  commandBarVisible = false
  globalShortcut.unregisterAll()
  BrowserWindow.getAllWindows().forEach((window) => {
    if (!window.isDestroyed()) window.hide()
  })
  app.quit()
  setTimeout(() => {
    app.exit(0)
  }, 500)
}

function buildTrayMenu(): Electron.Menu {
  return Menu.buildFromTemplate([
    {
      label: `TezBar ${app.getVersion()}`,
      enabled: false,
    },
    {
      label: 'Shortcut: Option+Space',
      enabled: false,
    },
    { type: 'separator' },
    {
      label: 'Show TezBar',
      click: () => openAppSurface('command'),
    },
    {
      label: 'Settings',
      click: () => openAppSurface('settings'),
    },
    {
      label: 'Clipboard History',
      click: () => openAppSurface('clipboard'),
    },
    { type: 'separator' },
    {
      label: 'Quit TezBar',
      click: () => {
        void confirmAndQuitRaymes()
      },
    },
  ])
}

function buildApplicationMenu(): Electron.Menu {
  const template: MenuItemConstructorOptions[] = [
    ...(process.platform === 'darwin'
      ? [
          {
            label: app.name,
            submenu: [
              { label: `About ${app.name}`, role: 'about' as const },
              { type: 'separator' as const },
              {
                label: 'Settings...',
                accelerator: 'CommandOrControl+,',
                click: () => openAppSurface('settings'),
              },
              { type: 'separator' as const },
              { label: `Hide ${app.name}`, role: 'hide' as const },
              { label: 'Hide Others', role: 'hideOthers' as const },
              { label: 'Show All', role: 'unhide' as const },
              { type: 'separator' as const },
              {
                label: `Quit ${app.name}`,
                accelerator: 'CommandOrControl+Q',
                click: () => {
                  void confirmAndQuitRaymes()
                },
              },
            ],
          },
        ]
      : []),
    {
      label: 'Edit',
      submenu: [
        { role: 'undo' },
        { role: 'redo' },
        { type: 'separator' },
        { role: 'cut' },
        { role: 'copy' },
        { role: 'paste' },
        { role: 'selectAll' },
      ],
    },
    {
      label: 'View',
      submenu: [
        { role: 'reload' },
        { role: 'forceReload' },
        { role: 'toggleDevTools' },
        { type: 'separator' },
        { role: 'resetZoom' },
        { role: 'zoomIn' },
        { role: 'zoomOut' },
      ],
    },
    {
      label: 'Window',
      submenu: [
        { role: 'minimize' },
        { role: 'zoom' },
        { type: 'separator' },
        {
          label: 'Show TezBar',
          accelerator: raymesHotkey,
          click: () => openAppSurface('command'),
        },
      ],
    },
    {
      label: 'Help',
      submenu: [
        {
          label: 'TezBar Settings',
          click: () => openAppSurface('settings'),
        },
      ],
    },
  ]

  return Menu.buildFromTemplate(template)
}

function handleRaymesHotkey(): void {
  if (raymesHotkey === DEFAULT_RAYMES_HOTKEY) {
    handleAltSpaceHotkey()
    return
  }
  toggleCommandBarImmediate()
}

function registerRaymesHotkey(accelerator: string): boolean {
  try {
    return globalShortcut.register(accelerator, handleRaymesHotkey)
  } catch {
    return false
  }
}

function updateRaymesHotkey(accelerator: string): {
  ok: boolean
  accelerator: string
  error?: string
} {
  const next = accelerator.trim()
  if (!next) {
    return { ok: false, accelerator: raymesHotkey, error: 'Press a valid shortcut.' }
  }
  if (next === raymesHotkey) {
    return { ok: true, accelerator: raymesHotkey }
  }

  const previous = raymesHotkey
  globalShortcut.unregister(previous)
  raymesHotkey = next

  if (!registerRaymesHotkey(next)) {
    raymesHotkey = previous
    registerRaymesHotkey(previous)
    return {
      ok: false,
      accelerator: previous,
      error: 'That shortcut is unavailable. It may already be used by another app.',
    }
  }

  stopFocusedAltSpaceGesture()
  stopAltSpaceHoldWatcher()
  setRaymesHotkey(next)
  Menu.setApplicationMenu(buildApplicationMenu())
  return { ok: true, accelerator: next }
}

function registerHotkey(): void {
  let okSpace = registerRaymesHotkey(raymesHotkey)
  if (!okSpace && raymesHotkey !== DEFAULT_RAYMES_HOTKEY) {
    console.warn(`Failed to register saved global shortcut ${raymesHotkey}; restoring default`)
    raymesHotkey = DEFAULT_RAYMES_HOTKEY
    okSpace = registerRaymesHotkey(raymesHotkey)
    setRaymesHotkey(raymesHotkey)
  }
  const okEnter = globalShortcut.register('Alt+Enter', toggleCommandBarImmediate)
  const okNote = globalShortcut.register('CommandOrControl+N', () => {
    if (!mainWindow || mainWindow.isDestroyed()) return
    showCommandBar()
    mainWindow.webContents.send('notes:quick-save-shortcut')
  })
  const okEscape = globalShortcut.register('CommandOrControl+Escape', () => {
    if (!mainWindow || mainWindow.isDestroyed() || !commandBarVisible) return
    hideCommandBar()
  })

  if (!okSpace) {
    console.warn(`Failed to register global shortcut ${raymesHotkey}`)
  }
  if (!okEnter) {
    console.warn('Failed to register global shortcut Alt+Enter')
  }
  if (!okNote) {
    console.warn('Failed to register global shortcut CommandOrControl+N (quick note)')
  }
  if (!okEscape) {
    console.warn('Failed to register global shortcut CommandOrControl+Escape (close window)')
  }
}

app.whenReady().then(() => {
  app.setName('TezBar')
  Menu.setApplicationMenu(buildApplicationMenu())

  // Chromium denies `getUserMedia` requests by default in Electron. The
  // launcher needs the mic for Hold-to-Speak; granting `media` here lets
  // the renderer call `navigator.mediaDevices.getUserMedia({ audio: true })`
  // without getting an instant NotAllowedError. The OS still prompts the
  // user via its native microphone sheet the first time — Electron just
  // needs to stop vetoing the request before it reaches the OS.
  // Note: Electron only declares 'media' in its permission enum (Chromium
  // decides between mic/camera/display inside the `media` umbrella and
  // exposes the specific kind via the request `details`). Approving
  // 'media' is enough for mic capture — the OS still shows its own
  // microphone consent sheet.
  session.defaultSession.setPermissionRequestHandler((_webContents, permission, callback) => {
    if (permission === 'media') {
      callback(true)
      return
    }
    callback(false)
  })
  session.defaultSession.setPermissionCheckHandler((_webContents, permission) => {
    return permission === 'media'
  })

  registerIpcHandlers(() => mainWindow, {
    startWindowDragMonitoring,
    stopWindowDragMonitoring,
    updateRaymesHotkey,
  })
  ipcMain.handle('settings:open-window', async () => {
    openSettingsWindow()
  })
  createWindow()
  placeWindow(mainWindow!)

  tray = new Tray(createTrayIcon())
  tray.setToolTip('TezBar')
  tray.setContextMenu(buildTrayMenu())
  tray.on('click', () => {
    if (isAppQuitting) return
    if (process.platform === 'darwin') {
      tray?.popUpContextMenu()
      return
    }
    showCommandBar()
  })

  registerHotkey()

  // Collect clipboard history in the background so the dedicated
  // clipboard view is useful even when the launcher has never been
  // opened in this session.
  startClipboardWatcher()

  app.on('activate', () => {
    if (isAppQuitting) return
    if (BrowserWindow.getAllWindows().length === 0) {
      createWindow()
    }
  })
})

app.on('before-quit', () => {
  isAppQuitting = true
  commandBarVisible = false
  globalShortcut.unregisterAll()
})

app.on('will-quit', () => {
  isAppQuitting = true
  stopFocusedAltSpaceGesture()
  stopAltSpaceHoldWatcher()
  globalShortcut.unregisterAll()
  tray?.destroy()
  tray = null
  stopClipboardWatcher()
  shutdownIpcHandlers()
  flushConfig()
  cleanupCenterOverlay()
})

app.on('window-all-closed', () => {
  if (process.platform !== 'darwin') {
    app.quit()
  }
})
