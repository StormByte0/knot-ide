/**
 * Monaco worker setup.
 *
 * Uses Vite's `?worker` suffix to import workers as ES modules. This is the
 * recommended approach for `@codingame/monaco-vscode-api` — do NOT use
 * `vite-plugin-monaco-editor`, it conflicts and causes build failures.
 */

import editorWorker from 'monaco-editor/esm/vs/editor/editor.worker?worker';
import jsonWorker from 'monaco-editor/esm/vs/language/json/json.worker?worker';
import cssWorker from 'monaco-editor/esm/vs/language/css/css.worker?worker';
import htmlWorker from 'monaco-editor/esm/vs/language/html/html.worker?worker';
import tsWorker from 'monaco-editor/esm/vs/language/typescript/ts.worker?worker';
// The TextMate worker ships in the monaco-vscode-textmate-service-override
// package, NOT in base monaco-editor.
import textmateWorker from '@codingame/monaco-vscode-textmate-service-override/worker?worker';

/**
 * Configure Monaco's worker resolution. Must be called before any editor or
 * service is created. Sets `self.MonacoEnvironment.getWorker`.
 */
export function setupMonacoWorkers(): void {
  self.MonacoEnvironment = {
    getWorker(_moduleId: string, label: string): Worker {
      switch (label) {
        case 'json':
          return new jsonWorker();
        case 'css':
        case 'scss':
        case 'less':
          return new cssWorker();
        case 'html':
        case 'handlebars':
        case 'razor':
          return new htmlWorker();
        case 'typescript':
        case 'javascript':
          return new tsWorker();
        case 'TextMateWorker':
          return new textmateWorker();
        default:
          return new editorWorker();
      }
    },
  };
}
