<script>
  // Inline rename field: Enter or blur commits, Escape cancels.
  let { value, oncommit, oncancel } = $props();

  let draft = $state(value);
  let done = false;

  function finish(commit) {
    if (done) return;
    done = true;
    const name = draft.trim();
    if (commit && name && name !== value) oncommit(name);
    else oncancel();
  }

  function onkeydown(e) {
    if (e.key === 'Enter') finish(true);
    else if (e.key === 'Escape') finish(false);
    e.stopPropagation(); // keep app shortcuts (Ctrl+N, …) out of the field
  }

  function select(node) {
    node.focus();
    node.select();
  }
</script>

<input
  class="name-input"
  type="text"
  spellcheck="false"
  aria-label="Note name"
  bind:value={draft}
  {onkeydown}
  onblur={() => finish(true)}
  use:select
/>

<style>
  .name-input {
    flex: 1;
    min-width: 0;
    padding: 2px 6px;
    font: inherit;
    color: var(--ink);
    background: var(--paper);
    border: 1px solid var(--accent);
    border-radius: 4px;
    outline: none;
  }
</style>
