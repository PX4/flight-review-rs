/// <reference types="@sveltejs/kit" />
declare namespace App {}

// Injected at build time by Vite `define` (see vite.config.ts).
declare const __APP_VERSION__: string;
declare const __GIT_SHA__: string;
declare const __BUILD_TIME__: string;
