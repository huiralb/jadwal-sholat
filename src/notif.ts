import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

interface Toast {
  title: string;
  body: string;
}

const judul = document.querySelector("#notif-judul") as HTMLElement;
const isi = document.querySelector("#notif-isi") as HTMLElement;

function render(toast: Toast): void {
  judul.textContent = toast.title;
  isi.textContent = toast.body;
}

async function init(): Promise<void> {
  await listen<Toast>("show-toast", (e) => render(e.payload));
  // Halaman bisa baru selesai dimuat saat notifikasi pertama tampil; tarik
  // payload terakhir agar tidak ada notifikasi yang hilang.
  try {
    const pending = await invoke<Toast | null>("get_toast");
    if (pending) render(pending);
  } catch {
    // Payload berikutnya datang lewat event.
  }
}

window.addEventListener("DOMContentLoaded", () => void init());
