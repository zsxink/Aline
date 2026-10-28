/* MarkFlow 风格的右键菜单与原生系统菜单栏预览。 */
(() => {
  'use strict';
  const $ = selector => document.querySelector(selector);
  const esc = value => String(value).replace(/[&<>"']/g, char => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[char]));
  const item = (label, shortcut, disabled = false, danger = false) => ({label, shortcut, disabled, danger});
  const sep = {divider:true};
  const submenu = (label, children) => ({label, submenu:children});
  const params = new URLSearchParams(location.search);
  const requestedPlatform = params.get('platform');
  const isMacHost = /Mac|iPhone|iPad/.test(navigator.platform || '');
  const platform = requestedPlatform === 'windows' ? 'windows' : requestedPlatform === 'macos' ? 'macos' : isMacHost ? 'macos' : 'windows';
  document.documentElement.dataset.platform = platform;

  const formatItems = [item('标题 1','⌘ 1'),item('标题 2','⌘ 2'),item('标题 3','⌘ 3'),item('标题 4','⌘ 4',true),item('标题 5','⌘ 5',true),item('标题 6','⌘ 6',true),item('正文','⌘ 0'),sep,item('粗体','⌘ B'),item('斜体','⌘ I'),item('行内代码','⌘ `'),item('高亮','',true),sep,item('删除线','⇧ ⌘ S'),item('数学公式（行内）'),item('注释（%%）','',true)];
  const menus = [
    {id:'app',label:'Aline',macOnly:true,items:[item('关于 Aline'),item('检查更新','',true),sep,item('设置…','⌘ ,'),sep,item('服务','',true),sep,item('隐藏 Aline','⌘ H'),item('隐藏其他','⌥ ⌘ H'),item('显示全部'),sep,item('退出 Aline','⌘ Q')]},
    {id:'file',label:'文件',items:[item('新建文件','⌘ N'),item('新建标签页','⌘ T'),item('新建文件夹','⇧ ⌘ N'),sep,item('打开文件…','⌘ O'),item('打开文件夹…'),item('关闭文件夹'),item('快速切换…','⌘ P',true),sep,item('保存','⌘ S'),item('另存为…'),sep,submenu('导出',[item('HTML'),item('PDF'),item('Word')]),sep,submenu('最近打开的文件',[item('01-基础语法.md'),item('04-公式与图表.md')]),submenu('最近打开的文件夹',[item('Markdown 示例')]),item('清除历史记录'),sep,item('关闭标签页','⌘ W'),item('关闭窗口','⇧ ⌘ W')]},
    {id:'edit',label:'编辑',items:[item('撤销','⌘ Z'),item('重做','⇧ ⌘ Z'),sep,item('剪切','⌘ X'),item('复制','⌘ C'),item('粘贴','⌘ V'),item('全选','⌘ A'),sep,item('查找…','⌘ F',true),item('替换…','⌥ ⌘ F',true)]},
    {id:'insert',label:'插入',items:[item('链接','⌘ K'),item('Markdown 链接','',true),item('标注（Callout）','',true),sep,item('代码块'),item('数学块'),item('表格'),item('脚注','',true),sep,item('无序列表'),item('有序列表'),item('任务列表','⌘ L'),sep,item('图片…（插入附件）'),item('折叠（Fold）','',true)]},
    {id:'format',label:'格式',items:formatItems},
    {id:'view',label:'视图',items:[item('切换所见即所得/源码','⌘ /'),item('切换侧边栏','⌘ \\'),item('显示文件树'),item('显示大纲'),sep,item('专注模式','⇧ ⌘ F'),sep,item('放大','⌘ =',true),item('缩小','⌘ -',true),item('重置缩放','',true),sep,item('全屏'),item('开发者工具','⌥ ⌘ I',true)]},
    {id:'theme',label:'主题',items:[]},
    {id:'window',label:'窗口',items:[item('最小化'),item('缩放'),sep,item('下一个标签页','Ctrl Tab'),item('上一个标签页','Ctrl ⇧ Tab'),item('恢复关闭的标签页','⇧ ⌘ T'),item('关闭所有标签页')]},
    {id:'help',label:'帮助',items:[item('快捷键一览'),item('打开日志目录'),item('问题反馈（GitHub Issue）'),...(platform==='windows'?[sep,item('关于 Aline')]:[])]}
  ];
  const shownMenus = menus.filter(menu => !menu.macOnly || platform === 'macos');
  const themeModes = [['light','白天'],['dark','夜间'],['sepia','护眼']];
  const themePalettes = [['github','GitHub'],['notion','Notion'],['vue','Vue'],['feishu','飞书']];
  function renderThemeMenu(){
    const currentMode=document.documentElement.dataset.theme||'light';
    const currentPalette=document.documentElement.dataset.palette||'github';
    const catalog=window.getThemeCatalog?.()||themePalettes.map(([id,name])=>({id,name}));
    const modes=themeModes.map(([mode,label])=>`<button class="app-menu-item app-theme-choice" type="button" role="menuitemradio" data-mode="${mode}" aria-checked="${mode===currentMode}"><span>${label}</span><span class="app-theme-check" aria-hidden="true">✓</span></button>`).join('');
    const palettes=catalog.map(theme=>`<button class="app-menu-item app-theme-choice" type="button" role="menuitemradio" data-palette="${esc(theme.id)}" aria-checked="${theme.id===currentPalette}"><span>${esc(theme.name)}</span><span class="app-theme-check" aria-hidden="true">✓</span></button>`).join('');
    return `<div class="app-theme-mode-list" role="group" aria-label="选择模式">${modes}</div><div class="app-menu-divider" role="separator"></div><div class="app-theme-palette-list" role="group" aria-label="所有主题">${palettes}</div>`;
  }
  const triggersMarkup = shownMenus.map(menu => `<button class="system-menu-trigger" type="button" data-menu="${menu.id}" aria-haspopup="menu" aria-expanded="false">${esc(menu.label)}</button>`).join('');
  if (platform === 'macos') {
    $('#system-menu-bar').innerHTML = `<div class="system-menu-group" role="menubar">${triggersMarkup}</div><div class="system-status-items" aria-hidden="true"><svg viewBox="0 0 16 16"><path d="M2 6a9 9 0 0 1 12 0M4.5 8.5a5.5 5.5 0 0 1 7 0M7 11a2 2 0 0 1 2 0M8 13.5v.1"/></svg><svg viewBox="0 0 16 16"><rect x="2.5" y="4" width="11" height="8" rx="2"/><path d="M5 6.5h6M5 9.5h6M14.5 6.5v3"/></svg><span>100%</span><span class="system-clock"></span></div>`;
  } else {
    const inline = document.createElement('div');
    inline.className = 'system-menu-inline';
    inline.setAttribute('role','menubar');
    inline.innerHTML = triggersMarkup;
    $('.titlebar').prepend(inline);
  }
  const clock = $('.system-clock');
  if (clock) clock.textContent = new Intl.DateTimeFormat('zh-CN',{hour:'2-digit',minute:'2-digit'}).format(new Date());

  const popup = document.createElement('div');
  popup.className = 'app-menu-popup';
  popup.hidden = true;
  popup.setAttribute('role','menu');
  document.body.append(popup);
  let activeTrigger = null;
  let returnFocus = null;
  const notify = message => {
    if (typeof window.toast === 'function') window.toast(message);
    else { const toast=$('#toast'); if(toast){toast.textContent=message;toast.hidden=false;setTimeout(()=>toast.hidden=true,1400);} }
  };
  function renderItems(items) {
    return items.map((entry,index) => {
      if (entry.divider) return '<div class="app-menu-divider" role="separator"></div>';
      const disabled = entry.disabled ? ' disabled aria-disabled="true"' : '';
      const submenuAttr = entry.submenu ? ' aria-haspopup="menu" aria-expanded="false"' : '';
      const row = `<button class="app-menu-item${entry.submenu?' has-submenu':''}${entry.danger?' danger':''}" role="menuitem" data-index="${index}"${disabled}${submenuAttr}><span>${esc(entry.label)}</span>${entry.shortcut?`<kbd>${esc(entry.shortcut)}</kbd>`:''}${entry.submenu?'<span class="app-menu-chevron">›</span>':''}</button>`;
      return entry.submenu ? `<div class="app-menu-entry">${row}<div class="app-menu-submenu" role="menu" hidden>${renderItems(entry.submenu)}</div></div>` : row;
    }).join('');
  }
  function closeMenu(restoreFocus = false) {
    const hadContextMenu = popup.classList.contains('context-menu-popup');
    popup.hidden = true;
    popup.replaceChildren();
    if (activeTrigger) activeTrigger.setAttribute('aria-expanded','false');
    activeTrigger = null;
    if (restoreFocus && returnFocus && (hadContextMenu || returnFocus.classList?.contains('system-menu-trigger'))) returnFocus.focus?.();
    returnFocus = null;
  }
  function openMenu(trigger) {
    const id = trigger.dataset.menu;
    const spec = shownMenus.find(menu => menu.id === id);
    if (!spec) return;
    if (activeTrigger === trigger && !popup.hidden) { closeMenu(); return; }
    closeMenu();
    popup.className = id==='theme'?'app-menu-popup theme-menu-popup':'app-menu-popup';
    activeTrigger = trigger;
    activeTrigger.setAttribute('aria-expanded','true');
    popup.innerHTML = id==='theme'?renderThemeMenu():renderItems(spec.items);
    popup.hidden = false;
    const rect = trigger.getBoundingClientRect();
    const width = popup.offsetWidth, height = popup.offsetHeight;
    popup.style.left = `${Math.max(8,Math.min(rect.left,innerWidth-width-8))}px`;
    popup.style.top = `${Math.max(8,Math.min(rect.bottom+2,innerHeight-height-8))}px`;
    returnFocus = trigger;
    popup.querySelector('.app-menu-item:not(:disabled)')?.focus();
  }
  function openSubmenu(entry) {
    const panel = entry.querySelector(':scope > .app-menu-submenu');
    if (!panel) return;
    entry.parentElement.querySelectorAll(':scope > .app-menu-entry > .app-menu-submenu').forEach(other => { if(other!==panel) other.hidden=true; });
    panel.hidden = false;
    entry.querySelector(':scope > .app-menu-item')?.setAttribute('aria-expanded','true');
    const rect=entry.getBoundingClientRect(), bounds=panel.getBoundingClientRect();
    const left=rect.right+bounds.width+8<innerWidth?rect.right-2:rect.left-bounds.width+2;
    panel.style.left=`${Math.max(8,left)}px`;
    panel.style.top=`${Math.max(8,Math.min(rect.top,innerHeight-bounds.height-8))}px`;
  }
  document.addEventListener('click', event => {
    const trigger = event.target.closest('.system-menu-trigger');
    if (trigger) { openMenu(trigger); return; }
    if (!event.target.closest('.app-menu-popup')) closeMenu();
  });
  document.addEventListener('mouseover', event => {
    const trigger = event.target.closest('.system-menu-trigger');
    if (trigger && activeTrigger && !popup.hidden && trigger!==activeTrigger) openMenu(trigger);
    const entry=event.target.closest('.app-menu-entry');
    if(entry) openSubmenu(entry);
  });
  document.addEventListener('click', event => {
    const row=event.target.closest('.app-menu-item');
    if (!row || row.disabled || row.closest('.context-menu-popup')) return;
    if (row.classList.contains('has-submenu')) { openSubmenu(row.parentElement); return; }
    const label=row.querySelector('span')?.textContent || '';
    closeMenu();
    if (row.dataset.mode) window.setAppearanceMode?.(row.dataset.mode,true);
    else if (row.dataset.palette) window.setPaletteFamily?.(row.dataset.palette);
    else if (label==='设置…') $('#settings-open')?.click();
    else if (label==='关于 Aline') window.openAboutDialog?.();
    else if (label==='快捷键一览') window.openShortcutsDialog?.();
    else if (label==='切换所见即所得/源码') $('#mode-source')?.click();
    else if (label==='切换侧边栏') $('#sidebar-toggle')?.click();
    else if (label==='专注模式') $('#focus-mode')?.click();
    else if (label==='新建标签页') $('#new-tab')?.click();
    else if (label==='关闭标签页') document.querySelector('.tab.active .tab-close')?.click();
    else notify(`${label}（原型操作）`);
  });
  document.addEventListener('keydown', event => { if (event.key==='Escape') closeMenu(true); });
  document.addEventListener('scroll', event => { if(event.target.closest?.('.app-theme-palette-list'))return; closeMenu(); }, true);

  const contextSpecs = {
    editor:[item('新增链接…'),sep,submenu('文本格式',[item('粗体','⌘ B'),item('斜体','⌘ I'),item('删除线'),item('行内代码')]),submenu('段落设置',[item('正文'),item('标题 1'),item('标题 2'),item('引用'),item('无序列表'),item('有序列表')]),submenu('插入',[item('链接…'),item('分隔线'),item('代码块'),item('图片…')]),sep,item('剪切','⌘ X'),item('复制','⌘ C'),item('粘贴','⌘ V'),item('以纯文本形式粘贴'),item('全选','⌘ A')],
    source:[item('剪切','⌘ X'),item('复制','⌘ C'),item('粘贴','⌘ V'),item('以纯文本形式粘贴'),sep,item('全选','⌘ A')],
    table:[item('在上方插入行'),item('在下方插入行'),item('在左侧插入列'),item('在右侧插入列'),sep,item('删除当前行'),item('删除当前列'),item('删除表格','',false,true),sep,item('左对齐'),item('居中对齐'),item('右对齐'),sep,item('表头行')],
    image:[item('复制到剪切板'),item('另存为'),sep,item('复制路径'),item('打开文件所在')],
    diagram:[item('图片另存为 SVG'),item('图片另存为 PNG'),sep,item('复制 SVG'),item('复制 PNG')],
    file:[item('新建文件'),item('新建文件夹'),sep,item('重命名'),item('复制（副本）'),item('删除','',false,true),sep,item('复制文件路径'),item('复制绝对路径'),sep,item('关闭文件夹'),item('在文件资源管理器中显示')],
    'file-empty':[item('新建文件'),item('新建文件夹'),sep,item('复制文件夹路径'),item('关闭文件夹'),item('在文件资源管理器中显示')],
    tab:[item('关闭标签页','⌘ W'),item('关闭其他标签页'),item('关闭所有标签页'),sep,item('恢复关闭的标签页','⇧ ⌘ T')]
  };
  function showContextMenu(kind,x,y,target) {
    closeMenu();
    popup.className = 'app-menu-popup context-menu-popup';
    let items = contextSpecs[kind];
    let hasSelection = false;
    if(kind==='source'){
      const textarea=target.closest('textarea');
      hasSelection=Boolean(textarea && textarea.selectionStart!==textarea.selectionEnd);
    }else if(kind==='editor'){
      hasSelection=Boolean(window.getSelection()?.toString());
    }
    if(kind==='source'||kind==='editor') items=items.map(entry => ['剪切','复制'].includes(entry.label)?{...entry,disabled:!hasSelection}:entry);
    popup.innerHTML = renderItems(items);
    returnFocus = target instanceof HTMLElement ? target : null;
    popup.hidden=false;
    const width=popup.offsetWidth,height=popup.offsetHeight;
    popup.style.left=`${Math.max(8,Math.min(x,innerWidth-width-8))}px`;
    popup.style.top=`${Math.max(8,Math.min(y,innerHeight-height-8))}px`;
    popup.querySelector('.app-menu-item:not(:disabled)')?.focus();
  }
  document.addEventListener('contextmenu', event => {
    const target=event.target instanceof Element?event.target:null;
    if(!target)return;
    let kind=null;
    if(target.closest('#side-content .tree-folder-row,#side-content .tree-demo-file')) kind='file';
    else if(target.closest('#side-content')) kind='file-empty';
    else if(target.closest('#tab-items .tab')) kind='tab';
    else if(target.closest('#editor table td,#editor table th')) kind='table';
    else if(target.closest('#editor img')) kind='image';
    else if(target.closest('#editor .mermaid-preview,#editor .diagram-preview,#editor .diagram,#editor .math-block')) kind='diagram';
    else if(target.closest('#editor')) kind=$('#editor')?.classList.contains('source-mode')?'source':'editor';
    if(!kind)return;
    event.preventDefault();
    showContextMenu(kind,event.clientX,event.clientY,target);
  });
  document.addEventListener('click', event => {
    const row=event.target.closest('.context-menu-popup .app-menu-item');
    if(!row||row.disabled)return;
    if(row.classList.contains('has-submenu')){openSubmenu(row.parentElement);return;}
    const label=row.querySelector('span')?.textContent||'';
    closeMenu();
    notify(`${label}（原型操作）`);
  });
  document.addEventListener('keydown', event => {
    if(popup.hidden || !popup.contains(document.activeElement))return;
    const keys=['ArrowDown','ArrowUp','Home','End'];
    if(keys.includes(event.key)){
      const rows=[...popup.querySelectorAll('.app-menu-item:not(:disabled)')].filter(button=>button.offsetParent!==null);
      if(!rows.length)return;
      event.preventDefault();
      const current=rows.indexOf(document.activeElement);
      const next=event.key==='Home'?0:event.key==='End'?rows.length-1:event.key==='ArrowDown'?(current+1)%rows.length:(current<=0?rows.length-1:current-1);
      rows[next].focus();
    } else if(event.key==='ArrowRight'){
      const row=document.activeElement.closest('.app-menu-item.has-submenu');
      if(row){event.preventDefault();openSubmenu(row.parentElement);row.parentElement.querySelector('.app-menu-submenu .app-menu-item:not(:disabled)')?.focus();}
    } else if(event.key==='ArrowLeft'){
      const panel=document.activeElement.closest('.app-menu-submenu');
      if(panel){event.preventDefault();panel.hidden=true;panel.parentElement.querySelector(':scope > .app-menu-item')?.focus();}
    } else if(event.key==='Enter' || event.key===' '){
      const row=document.activeElement.closest('.app-menu-item');
      if(row){event.preventDefault();row.click();}
    }
  });
})();
