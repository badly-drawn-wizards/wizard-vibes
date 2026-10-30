// Status line and compiler-error overlay.

const status = document.getElementById("status")!;
const error = document.getElementById("error")!;

export function setStatus(text: string): void {
  status.textContent = text;
}

export function showError(text: string): void {
  error.textContent = text;
  error.hidden = false;
}

export function clearError(): void {
  error.hidden = true;
}
