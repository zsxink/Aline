/* 统一应用壳图标的尺寸、描边与可访问名称。 */
(() => {
if (typeof settings !== 'undefined') {
  const template=document.createElement('template');template.innerHTML=settings['高级'];
  template.content.querySelectorAll('.settings-group').forEach(group=>{if(group.querySelector('h3')?.textContent==='PlantUML')group.remove()});
  settings['高级']=template.innerHTML.replace('调整文件树、文档处理与图表服务。','调整文件树与文档处理。');
}
const paths={
 'file':'<path d="M14 2H6a2 2 0 0 0-2 2v16h16V8zM14 2v6h6"/>',
 'folder':'<path d="M3 7V5h7l2 2h9v13H3z"/>',
 'plus':'<path d="M12 5v14M5 12h14"/>', 'close':'<path d="m6 6 12 12M6 18 18 6"/>',
 'chevron-down':'<path d="m6 9 6 6 6-6"/>','chevron-right':'<path d="m9 6 6 6-6 6"/>',
 'check':'<path d="m5 12 4 4L19 6"/>',
 'sidebar':'<rect x="3" y="4" width="18" height="16" rx="2"/><path d="M9 4v16"/>',
 'bold':'<path d="M6 4h7a4 4 0 0 1 0 8H6V4zm0 8h8a4 4 0 0 1 0 8H6z"/>',
 'italic':'<path d="M10 4h10M4 20h10M15 4 9 20"/>',
 'strike':'<path d="M17 5c-2-2-8-2-9 1-1 2 1 4 4 5M7 18c3 3 9 2 10-1M3 12h18"/>',
 'ordered':'<path d="M10 6h11M10 12h11M10 18h11M3 3h1v5M2 8h4M2 12c0-2 4-2 4 0 0 1-4 3-4 5h4"/>',
 'quote':'<path d="M4 5h6v7H5c0 4-1 5-2 6M15 5h6v7h-5c0 4-1 5-2 6"/>',
 'sun':'<circle cx="12" cy="12" r="4"/><path d="M12 2v2m0 16v2M2 12h2m16 0h2M5 5l1 1m12 12 1 1M5 19l1-1M18 6l1-1"/>',
 'moon':'<path d="M20 14A8 8 0 0 1 10 4a8 8 0 1 0 10 10z"/>',
 'eye':'<path d="M2 12s4-7 10-7 10 7 10 7-4 7-10 7S2 12 2 12z"/><circle cx="12" cy="12" r="3"/>',
 'focus':'<path d="M9 3H3v6m12-6h6v6M3 15v6h6m12-6v6h-6"/>',
 'samples':'<rect x="4" y="3" width="16" height="18" rx="2"/><path d="M8 7h8M8 12h8M8 17h5"/>',
 'settings':'<path d="M4 7h16M4 17h16M8 4v6m8 4v6"/>',
 'refresh':'<path d="M20 10a8 8 0 0 0-14-4L3 9m0-6v6h6M4 14a8 8 0 0 0 14 4l3-3m0 6v-6h-6"/>'
};
window.alineIcon=name=>`<svg viewBox="0 0 24 24" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round">${paths[name]||paths.file}</svg>`;
const put=(selector,name)=>{const e=document.querySelector(selector);if(e)e.innerHTML=window.alineIcon(name)};
put('#sidebar-toggle','sidebar');put('[title="粗体"]','bold');put('[title="斜体"]','italic');put('[title="删除线"]','strike');put('[title="有序列表"]','ordered');put('[title="引用"]','quote');put('#status-settings','settings');put('#status-focus','focus');put('#focus-mode','focus');put('#theme-cycle','sun');put('#new-tab','plus');put('.tab-list','chevron-down');put('#export-btn','samples');
const samples=document.querySelector('#export-btn');samples.title='Markdown 示例';samples.onclick=()=>window.alinePrototype?.openDocument(window.ALINE_DEMOS[0].id);
document.querySelector('[title="新增 Frontmatter"]')?.remove();
document.querySelector('#brand-menu span').innerHTML=window.alineIcon('chevron-down');
document.querySelector('.titlebar').insertAdjacentHTML('afterbegin','<span class="traffic" aria-hidden="true"><i></i><i></i><i></i></span>');
document.querySelectorAll('button').forEach(b=>{if(b.title)b.setAttribute('aria-label',b.title)});
document.querySelector('#status-settings').setAttribute('aria-label','打开设置');document.querySelector('#status-focus').setAttribute('aria-label','专注模式');document.querySelector('#new-tab').setAttribute('aria-label','新建标签');
document.querySelector('#theme-cycle').onclick=()=>window.cycleAppearanceMode?.();document.querySelector('#theme-cycle').title='切换主题';document.querySelector('#theme-cycle').setAttribute('aria-label','切换主题');
})();
