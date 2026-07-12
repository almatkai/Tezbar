import type { Rectangle } from '@tezbar/desktop-runtime'

let suppressBlurHide = false
let windowBounds: Rectangle | null = null

export function setSuppressBlurHide(value: boolean): void {
  suppressBlurHide = value
}

export function shouldSuppressBlurHide(): boolean {
  return suppressBlurHide
}

export function setWindowBounds(bounds: Rectangle): void {
  windowBounds = bounds
}

export function getWindowBounds(): Rectangle | null {
  return windowBounds
}
