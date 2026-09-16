# Toolbar icons

Use official [Lucide](https://github.com/lucide-icons/lucide) icons for every
toolbar control. Do not draw custom replacements or mix in another icon set.
Preserve Lucide's 24-by-24 viewBox, 2-unit stroke and rounded caps/joins; toolbar
size and theme color are applied by the existing GPUI presentation.

The SVGs here are unmodified upstream files from revision
[`076b52527f0c5fe4cc1cd2472ef716fc332ccf0e`](https://github.com/lucide-icons/lucide/tree/076b52527f0c5fe4cc1cd2472ef716fc332ccf0e/icons).
Their local filenames preserve the application's asset paths.
The upstream [license](LICENSE) includes the Lucide and Feather notices.

| Local SVG | Lucide icon |
| --- | --- |
| bold.svg | bold |
| italic.svg | italic |
| strikethrough.svg | strikethrough |
| heading.svg | heading |
| inlinecode.svg | code |
| codeblock.svg | square-code |
| link.svg | link |
| blockquote.svg | text-quote |
| orderedlist.svg | list-ordered |
| unorderedlist.svg | list |
| tasklist.svg | list-todo |
| sort.svg | arrow-up-narrow-wide |

Back, forward, sidebar open/close, folder and search reuse the Lucide SVGs
already embedded in `gpui-component-assets` 0.5.1. Sort uses the upstream icon
here because the component's `SortAscending` is a modified chevrons icon.

For additions, reuse a component icon when it is an official Lucide asset;
otherwise copy the upstream SVG here, register it in `src/assets.rs`, and record
its name and revision in this table. Keep the upstream license alongside it.
