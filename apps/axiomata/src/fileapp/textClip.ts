/**
 * Where the editor's text layer is cut (`EditorSurface.svelte`, docs/plans/glass.md G18): at the gutter's right edge,
 * so text scrolled under the sticky gutter is not there at all — on glass the gutter would otherwise show it. Only the
 * left side is cut: an inline diagnostic, a fold pill or a line label may reach past the layer's own right and bottom
 * edge, so those insets lie a whole viewport out.
 */
export function textClipPath(scrollLeft: number, gutterW: number, textLeft: number): string {
  return `inset(0 -100vw -100vh ${scrollLeft + gutterW - textLeft}px)`;
}
