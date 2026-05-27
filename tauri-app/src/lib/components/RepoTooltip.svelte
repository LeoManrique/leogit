<script lang="ts">
  // Tiny floating tooltip used by anything that wants to show
  // "<bold identifier>\n<dim on-disk path>" on hover. Positioned fixed in
  // viewport coordinates so it can break out of any overflow:hidden parent
  // (the repo dropdown clips otherwise).
  interface Props {
    title: string
    path: string
    x: number
    y: number
  }
  let { title, path, x, y }: Props = $props()
</script>

<div class="repo-tooltip" style="left: {x}px; top: {y}px;">
  <div class="repo-tooltip-title">{title}</div>
  <div class="repo-tooltip-path">{path}</div>
</div>

<style>
  .repo-tooltip {
    position: fixed;
    z-index: 1100;
    background: var(--bg-elevated);
    border: 1px solid var(--border-inactive);
    border-radius: 6px;
    box-shadow: var(--shadow-popover);
    padding: 6px 10px;
    pointer-events: none;
    max-width: 420px;
  }

  .repo-tooltip-title {
    font-size: 12px;
    font-weight: 600;
    color: var(--text-primary);
  }

  .repo-tooltip-path {
    font-size: 11px;
    color: var(--text-muted);
    margin-top: 2px;
    word-break: break-all;
  }
</style>
