import "node:module";
import { svelte } from "file:///Users/ciarankelly/Documents/side-projects/codex-custom/node_modules/.deno/@sveltejs+vite-plugin-svelte@7.2.0/node_modules/@sveltejs/vite-plugin-svelte/src/index.js";
import tailwindcss from "file:///Users/ciarankelly/Documents/side-projects/codex-custom/node_modules/.deno/@tailwindcss+vite@4.3.3/node_modules/@tailwindcss/vite/dist/index.mjs";
import { defineConfig } from "file:///Users/ciarankelly/Documents/side-projects/codex-custom/node_modules/.deno/vite@8.1.5/node_modules/vite/dist/node/index.js";
const __vite_injected_original_import_meta_url = "file:///Users/ciarankelly/Documents/side-projects/codex-custom/vite.config.ts";
const host = globalThis.process?.env.TAURI_DEV_HOST;
var vite_config_default = defineConfig({
	plugins: [tailwindcss(), svelte()],
	resolve: { alias: { $lib: new URL("./src/lib", __vite_injected_original_import_meta_url).pathname } },
	clearScreen: false,
	server: {
		host: host || false,
		port: 1420,
		strictPort: true,
		hmr: host ? {
			protocol: "ws",
			host,
			port: 1421
		} : void 0,
		watch: { ignored: ["**/src-tauri/**"] }
	}
});
//#endregion
export { vite_config_default as default };

//# sourceMappingURL=data:application/json;charset=utf-8;base64,eyJ2ZXJzaW9uIjozLCJmaWxlIjoidml0ZS5jb25maWcuanMiLCJuYW1lcyI6W10sInNvdXJjZXMiOlsiL1VzZXJzL2NpYXJhbmtlbGx5L0RvY3VtZW50cy9zaWRlLXByb2plY3RzL2NvZGV4LWN1c3RvbS92aXRlLmNvbmZpZy50cyJdLCJzb3VyY2VzQ29udGVudCI6WyJpbXBvcnQgeyBzdmVsdGUgfSBmcm9tIFwiQHN2ZWx0ZWpzL3ZpdGUtcGx1Z2luLXN2ZWx0ZVwiO1xuaW1wb3J0IHRhaWx3aW5kY3NzIGZyb20gXCJAdGFpbHdpbmRjc3Mvdml0ZVwiO1xuaW1wb3J0IHsgZGVmaW5lQ29uZmlnIH0gZnJvbSBcInZpdGVcIjtcblxuY29uc3QgaG9zdCA9IChcbiAgZ2xvYmFsVGhpcyBhcyB0eXBlb2YgZ2xvYmFsVGhpcyAmIHtcbiAgICBwcm9jZXNzPzogeyBlbnY6IFJlY29yZDxzdHJpbmcsIHN0cmluZyB8IHVuZGVmaW5lZD4gfTtcbiAgfVxuKS5wcm9jZXNzPy5lbnYuVEFVUklfREVWX0hPU1Q7XG5cbmV4cG9ydCBkZWZhdWx0IGRlZmluZUNvbmZpZyh7XG4gIHBsdWdpbnM6IFt0YWlsd2luZGNzcygpLCBzdmVsdGUoKV0sXG4gIHJlc29sdmU6IHtcbiAgICBhbGlhczogeyAkbGliOiBuZXcgVVJMKFwiLi9zcmMvbGliXCIsIGltcG9ydC5tZXRhLnVybCkucGF0aG5hbWUgfSxcbiAgfSxcbiAgY2xlYXJTY3JlZW46IGZhbHNlLFxuICBzZXJ2ZXI6IHtcbiAgICBob3N0OiBob3N0IHx8IGZhbHNlLFxuICAgIHBvcnQ6IDE0MjAsXG4gICAgc3RyaWN0UG9ydDogdHJ1ZSxcbiAgICBobXI6IGhvc3QgPyB7IHByb3RvY29sOiBcIndzXCIsIGhvc3QsIHBvcnQ6IDE0MjEgfSA6IHVuZGVmaW5lZCxcbiAgICB3YXRjaDogeyBpZ25vcmVkOiBbXCIqKi9zcmMtdGF1cmkvKipcIl0gfSxcbiAgfSxcbn0pO1xuIl0sIm1hcHBpbmdzIjoiOzs7O0FBQXNOLE1BQU0sMkNBQTJDO0FBSXZRLE1BQU0sT0FDSixXQUdBLFNBQVMsSUFBSTtBQUVmLElBQUEsc0JBQWUsYUFBYTtDQUMxQixTQUFTLENBQUMsWUFBWSxHQUFHLE9BQU8sQ0FBQztDQUNqQyxTQUFTLEVBQ1AsT0FBTyxFQUFFLE1BQU0sSUFBSSxJQUFJLGFBQUEsd0NBQTRCLENBQUMsQ0FBQyxTQUFTLEVBQ2hFO0NBQ0EsYUFBYTtDQUNiLFFBQVE7RUFDTixNQUFNLFFBQVE7RUFDZCxNQUFNO0VBQ04sWUFBWTtFQUNaLEtBQUssT0FBTztHQUFFLFVBQVU7R0FBTTtHQUFNLE1BQU07RUFBSyxJQUFJO0VBQ25ELE9BQU8sRUFBRSxTQUFTLENBQUMsaUJBQWlCLEVBQUU7Q0FDeEM7QUFDRixDQUFDIn0=