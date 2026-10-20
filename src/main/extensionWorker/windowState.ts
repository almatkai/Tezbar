import { notify } from './host'

export function setSuppressBlurHide(value: boolean): void {
  notify('core:platform:suppress-blur', value)
}
