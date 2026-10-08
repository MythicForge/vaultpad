<script>
  import { onDestroy, untrack } from 'svelte';
  import { minimalSetup } from 'codemirror';
  import { EditorState } from '@codemirror/state';
  import { EditorView, keymap, placeholder } from '@codemirror/view';
  import { indentWithTab } from '@codemirror/commands';
  import { markdown } from '@codemirror/lang-markdown';

  // `loadKey` changes whenever the parent wants the document replaced
  // (switching notes, external edits). Normal typing never resets the editor.
  let { doc, loadKey, onchange } = $props();

  let host;
  let view;

  // drawSelection (in minimalSetup) hides the native caret and paints its own
  // cursor/selection with light-only defaults; recolor them from app tokens.
  const tokenTheme = EditorView.theme({
    '.cm-cursor, .cm-dropCursor': { borderLeftColor: 'var(--ink)', borderLeftWidth: '2px' },
    '&.cm-focused > .cm-scroller > .cm-selectionLayer .cm-selectionBackground, .cm-selectionBackground, .cm-content ::selection':
      { backgroundColor: 'var(--selection) !important' },
  });

  function makeState(text) {
    return EditorState.create({
      doc: text,
      extensions: [
        minimalSetup,
        markdown(),
        EditorView.lineWrapping,
        tokenTheme,
        keymap.of([indentWithTab]),
        placeholder('Start typing… first line becomes the file name'),
        EditorView.updateListener.of((u) => {
          if (u.docChanged) onchange(u.state.doc.toString());
        }),
      ],
    });
  }

  $effect(() => {
    loadKey;
    const text = untrack(() => doc);
    if (!view) view = new EditorView({ parent: host, state: makeState(text) });
    else view.setState(makeState(text));
    view.focus();
  });

  onDestroy(() => view?.destroy());
</script>

<div class="editor" bind:this={host}></div>

<style>
  .editor {
    flex: 1;
    min-height: 0;
    display: flex;
  }
  .editor :global(.cm-editor) {
    flex: 1;
    height: 100%;
    font-size: 15px;
    background: var(--paper);
    color: var(--ink);
  }
  .editor :global(.cm-editor.cm-focused) {
    outline: none;
  }
  .editor :global(.cm-scroller) {
    font-family: var(--mono);
    line-height: 1.6;
    padding: 16px 20px;
  }
  .editor :global(.cm-placeholder) {
    color: var(--muted);
  }
</style>
