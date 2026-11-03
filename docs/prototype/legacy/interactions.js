/* 原型交互层：每个标签持有独立文本，浏览器会话保存不写入磁盘。 */
(() => {
  'use strict';
  const q = s => document.querySelector(s);
  const qa = s => [...document.querySelectorAll(s)];
  const escape = value => String(value).replace(/[&<>"']/g, char => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[char]));
  const svg = path => `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${path}</svg>`;
  const paths = {
    file:'<path d="M14 3H6a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V9zM14 3v6h6"/>',
    folder:'<path d="M3 7a2 2 0 0 1 2-2h5l2 2h7a2 2 0 0 1 2 2v10H3z"/>',
    down:'<path d="m7 10 5 5 5-5"/>', right:'<path d="m10 7 5 5-5 5"/>',
    plus:'<path d="M12 5v14M5 12h14"/>', close:'<path d="m7 7 10 10M17 7 7 17"/>',
    filePlus:'<path d="M14 3H6a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h8M14 3v6h6M17 14v7m-3.5-3.5h7"/>',
    folderPlus:'<path d="M3 7a2 2 0 0 1 2-2h5l2 2h7a2 2 0 0 1 2 2v5M3 7v10a2 2 0 0 0 2 2h8m2-1h7m-3.5-3.5v7"/>',
    expand:'<path d="m7 7 5 5 5-5m-10 6 5 5 5-5"/>', collapse:'<path d="m7 17 5-5 5 5m-10-6 5-5 5 5"/>',
    refresh:'<path d="M20 11a8 8 0 0 0-14-5L4 8m0-5v5h5M4 13a8 8 0 0 0 14 5l2-2m0 5v-5h-5"/>'
  };
  const icon = (name,className='') => { const markup=svg(paths[name] || paths.file); return className?markup.replace('<svg ','<svg class="'+className+'" '):markup; };
  const demos = (window.ALINE_DEMOS || []).map(d => ({...d, dirty:false, mode:'visual', scroll:0}));
  if (!demos.length) return;
  const storageKey = 'aline-prototype-workspace-v2';
  let documents = [structuredClone(demos[0])];
  let active = documents[0].id;
  let sidebarMode = 'files';
  let folderCount = 0;
  const workspaceNodes = [
    {type:'folder',id:'basics',name:'01-基础语法',children:[{type:'demo',id:'overview'},{type:'demo',id:'lists'}]},
    {type:'folder',id:'code',name:'02-代码与表格',children:[{type:'demo',id:'code'}]},
    {type:'folder',id:'math',name:'03-公式与图表',children:[{type:'demo',id:'math'}]},
    {type:'folder',id:'media',name:'04-图片与HTML',children:[{type:'demo',id:'html'},{type:'folder',id:'media-images',name:'images',children:[{type:'asset',id:'landscape',name:'showcase-landscape.svg'}]}]},
    {type:'demo',id:'readme'}
  ];
  const expandedFolders = new Set(['basics','code','media','media-images']);
  let untitledCount = 0;
  let sourceChanged = false;
  let editor = q('#editor');
  const replacement = editor.cloneNode(false);
  editor.replaceWith(replacement);
  editor = replacement;
  editor.setAttribute('aria-label', 'Markdown 文档');
  try {
    const saved = JSON.parse(localStorage.getItem(storageKey));
    if (saved && Array.isArray(saved.documents)) {
      documents = saved.documents.filter(d => d && typeof d.id === 'string' && typeof d.source === 'string' && typeof d.html === 'string');
      documents = documents.map(doc => {
        const demo = demos.find(item => item.id === doc.id);
        if (!demo) return doc;
        if (/代码语言|更改代码语言|复制代码|viewBox=\"0 0 24 24\"|textbashccppcssgohtml/.test(doc.source)) {
          const savedHtml=document.createElement('template'); savedHtml.innerHTML=doc.html;
          const languages=[...savedHtml.content.querySelectorAll('.code-language')].map(select=>select.value);
          let index=0, source=demo.source.replace(/^(\s*`{3,})([\w+-]*)/gm,(match,fence)=>`${fence}${languages[index++]||match.slice(fence.length)}`);
          return {...doc,source,html:demo.html,dirty:source!==demo.source};
        }
        return !doc.dirty ? {...doc, source:demo.source, html:demo.html} : doc;
      });
      active = documents.some(d => d.id === saved.active) ? saved.active : documents[0]?.id || null;
    }
  } catch (_) { /* 无法读取存储时直接使用内置示例。 */ }
  const current = () => documents.find(d => d.id === active) || documents[0];
  const notify = text => typeof window.toast === 'function' ? window.toast(text) : undefined;
  function remember() {
    try { localStorage.setItem(storageKey, JSON.stringify({documents,active})); }
    catch (_) { notify('浏览器存储空间不足，请复制内容后保存。'); }
  }
  function stats() {
    const doc = current();
    const source = doc?.source || '';
    q('#words').textContent = `${[...source.replace(/\s/g,'')].length} 字`;
    q('#lines').textContent = `${Math.max(1,source.split('\n').length)} 行`;
    q('#cursor').textContent = '行 1, 列 0';
    const input = editor.querySelector('textarea');
    if (input) {
      const before = input.value.slice(0,input.selectionStart).split('\n');
      q('#cursor').textContent = `行 ${before.length}, 列 ${[...before.at(-1)].length}`;
    }
  }
  function capture() {
    const doc = current();
    if (!doc) return;
    doc.scroll = q('.editor-area').scrollTop;
    const input = editor.querySelector('textarea');
    if (input) {
      if (input.value !== doc.source) { doc.source = input.value; doc.html = renderMarkdown(doc.source); doc.dirty = true; }
    } else {
      doc.html = editor.innerHTML;
    }
  }
  function renderTabs() {
    const tabs = q('#tab-items');
    tabs.querySelectorAll('.tab').forEach(el => el.remove());
    documents.forEach(doc => {
      const tab = document.createElement('div');
      tab.className = `tab${doc.id === active ? ' active' : ''}`;
      tab.dataset.id = doc.id;
      tab.setAttribute('role','tab');
      tab.setAttribute('aria-selected',String(doc.id === active));
      tab.tabIndex = 0;
      tab.innerHTML = `<span class="tab-title">${escape(doc.name)}</span>${doc.dirty?'<span class="tab-dirty" aria-label="未保存">•</span>':''}<button class="tab-close" aria-label="关闭 ${escape(doc.name)}">${icon('close')}</button>`;
      tab.onclick = event => event.target.closest('.tab-close') ? closeDocument(doc.id) : switchDocument(doc.id);
      tab.onkeydown = event => { if (event.key === 'Enter') switchDocument(doc.id); };
      tabs.append(tab);
    });
    q('#new-tab').innerHTML = icon('plus');
    q('#new-tab').setAttribute('aria-label','新建标签');
    q('.tab-list').innerHTML = icon('down');
    const strip = q('#tab-strip');
    const selected = tabs.querySelector('.tab.active');
    if (selected && strip) {
      const left = selected.offsetLeft;
      const right = left + selected.offsetWidth;
      if (left < strip.scrollLeft) strip.scrollLeft = left;
      else if (right > strip.scrollLeft + strip.clientWidth) strip.scrollLeft = right - strip.clientWidth;
    }
  }
  function renderTree() {
    const container = q('#side-content');
    if (sidebarMode === 'outline') { renderOutline(); return; }
    const orderedNodes = nodes => [...nodes].sort((a,b) => Number(a.type!=='folder') - Number(b.type!=='folder'));
    const renderNode = node => {
      if (node.type === 'folder') {
        const open = expandedFolders.has(node.id);
        return `<div class="tree-folder"><button class="file-row tree-folder-row" data-folder="${escape(node.id)}" aria-expanded="${open}">${icon(open?'down':'right','tree-chevron')}${icon('folder','folder-icon')}<span>${escape(node.name)}</span></button><div class="tree-nested"${open?'':' hidden'}>${orderedNodes(node.children||[]).map(renderNode).join('')}</div></div>`;
      }
      const doc = node.type === 'demo' ? demos.find(item => item.id === node.id) : null;
      const name = doc?.name || node.name;
      const selected = doc?.id === active || node.docId === active;
      const kind = node.type==='demo'||node.type==='new-file' ? 'markdown' : /\.(png|jpe?g|gif|webp|svg|bmp)$/i.test(name) ? 'image' : 'other';
      return `<button class="file-row tree-demo-file${selected?' selected':''}" data-kind="${kind}" ${node.type==='demo'?`data-demo="${escape(node.id)}"`:node.type==='new-file'?`data-document="${escape(node.docId)}"`:`data-asset="${escape(node.name)}"`}>${icon('file','tree-icon')}<span>${escape(name)}</span></button>`;
    };
    const allFolderIds = [];
    const collectFolders = nodes => nodes.forEach(node => { if (node.type==='folder') { allFolderIds.push(node.id); collectFolders(node.children||[]); } });
    collectFolders(workspaceNodes);
    const anyExpanded = allFolderIds.some(id => expandedFolders.has(id));
    container.innerHTML = `<div class="tree-header"><span class="tree-header-name">Markdown 示例</span><div class="tree-header-actions"><button class="tree-header-action" data-tree-action="new-file" title="新建文件" aria-label="新建文件">${icon('filePlus')}</button><button class="tree-header-action" data-tree-action="new-folder" title="新建文件夹" aria-label="新建文件夹">${icon('folderPlus')}</button><button class="tree-header-action" data-tree-action="refresh" title="刷新" aria-label="刷新">${icon('refresh')}</button><button class="tree-header-action" data-tree-action="toggle-folders" title="${anyExpanded?'收起':'展开'}所有子文件夹" aria-label="${anyExpanded?'收起':'展开'}所有子文件夹">${icon(anyExpanded?'collapse':'expand')}</button></div></div><div class="tree-children">${orderedNodes(workspaceNodes).map(renderNode).join('')}</div>`;
    container.querySelector('[data-tree-action="new-file"]').onclick = createTreeFile;
    container.querySelector('[data-tree-action="new-folder"]').onclick = createTreeFolder;
    container.querySelector('[data-tree-action="refresh"]').onclick = () => renderTree();
    container.querySelector('[data-tree-action="toggle-folders"]').onclick = () => { if (anyExpanded) expandedFolders.clear(); else allFolderIds.forEach(id=>expandedFolders.add(id)); renderTree(); };
    container.querySelectorAll('[data-folder]').forEach(row => row.onclick = () => { const id=row.dataset.folder; expandedFolders.has(id)?expandedFolders.delete(id):expandedFolders.add(id); renderTree(); });
    container.querySelectorAll('[data-demo]').forEach(row => row.onclick = () => openDemo(row.dataset.demo));
    container.querySelectorAll('[data-document]').forEach(row => row.onclick = () => switchDocument(row.dataset.document));
    container.querySelectorAll('[data-asset]').forEach(row => row.onclick = () => notify(`示例素材：${row.dataset.asset}`));
  }
  function createTreeFile() {
    newDocument();
    const doc = current();
    workspaceNodes.push({type:'new-file',docId:doc.id,name:`${doc.name}.md`});
    renderTree();
  }
  function createTreeFolder() {
    folderCount += 1;
    const id = `new-folder-${folderCount}`;
    workspaceNodes.push({type:'folder',id,name:`新建文件夹 ${folderCount}`,children:[]});
    expandedFolders.add(id);
    renderTree();
  }
  function renderOutline() {
    const headings = [...editor.querySelectorAll('h1,h2,h3,h4,h5,h6')];
    const container = q('#side-content');
    if (!headings.length) {
      container.innerHTML = '<div class="outline-empty" style="padding:18px 16px;color:var(--muted);font-size:12px">当前文档无标题</div>';
      return;
    }
    container.innerHTML = headings.map((heading,index) => `<button class="outline-row outline-item" data-heading="${index}" style="padding-left:${16+(Number(heading.tagName.slice(1))-1)*12}px">${escape(heading.textContent)}</button>`).join('');
    container.querySelectorAll('[data-heading]').forEach(button => button.onclick = () => headings[Number(button.dataset.heading)].scrollIntoView({behavior:'smooth',block:'start'}));
  }
  const codeLanguages = ['text','bash','c','cpp','csharp','css','diff','go','graphql','html','java','javascript','json','kotlin','markdown','objective-c','php','python','ruby','rust','shell','sql','swift','typescript','tsx','xml','yaml'];
  const languageAliases = {typescript:'ts',javascript:'js','objective-c':'objc',plaintext:'text'};
  const languageLabel = language => languageAliases[language] || language;
  const copyIcon = svg('<rect x="8" y="8" width="13" height="13" rx="2"/><path d="M16 8V4a2 2 0 0 0-2-2H4a2 2 0 0 0-2 2v10a2 2 0 0 0 2 2h4"/>');
  const copiedIcon = svg('<path d="m5 12 4 4L19 6"/>');
  function showCopied(button) {
    button.innerHTML = copiedIcon;
    button.classList.add('copied');
    button.setAttribute('aria-label','已复制');
    button.title = '已复制';
    clearTimeout(button.copyResetTimer);
    button.copyResetTimer = setTimeout(()=>{
      button.innerHTML = copyIcon;
      button.classList.remove('copied');
      button.setAttribute('aria-label','复制代码');
      button.title = '复制代码';
    },2000);
  }
  function refreshLineNumbers(pre) {
    const code = pre.querySelector('code');
    const gutter = pre.querySelector('.code-gutter');
    if (!code || !gutter) return;
    const source = code.textContent.replace(/\n$/, '');
    gutter.textContent = Array(Math.max(1, source.split('\n').length)).fill(0).map((_,index)=>index+1).join('\n');
  }
  function decorateCodeBlocks() {
    editor.querySelectorAll('pre').forEach(pre => {
      let wrapper = pre.closest('.code-block');
      if (!wrapper) {
        wrapper = document.createElement('div'); wrapper.className = 'code-block';
        pre.before(wrapper); wrapper.append(pre);
      }
      const oldToolbar = wrapper.querySelector('.code-toolbar');
      const oldSelect = oldToolbar?.querySelector('.code-language');
      const oldHeader = wrapper.querySelector(':scope > header');
      const rawLabel = oldSelect?.value || wrapper.dataset.language || oldHeader?.textContent.trim() || pre.dataset.language || pre.dataset.sourceLanguage || 'text';
      const label = rawLabel.toLowerCase();
      oldToolbar?.remove(); oldHeader?.remove(); wrapper.classList.remove('code-block-plain');
      wrapper.dataset.language = label;
      pre.querySelector('.code-gutter')?.remove();
      const gutter = document.createElement('span');
      gutter.className = 'code-gutter'; gutter.setAttribute('aria-hidden','true');
      pre.prepend(gutter); refreshLineNumbers(pre);
      const toolbar = document.createElement('div');
      toolbar.className = 'code-toolbar';
      const options = [...new Set(['text',...codeLanguages,label])].map(language => '<button type="button" class="code-language-option' + (language===label?' selected':'') + '" role="option" aria-selected="' + (language===label) + '" data-language="' + escape(language) + '">' + escape(language) + '</button>').join('');
      toolbar.innerHTML = '<button class="code-language-trigger" type="button" aria-label="更改代码语言" aria-haspopup="listbox" aria-expanded="false"><span class="code-language-value">' + escape(languageLabel(label)) + '</span>' + icon('down') + '</button><button class="copy-code" type="button" aria-label="复制代码" title="复制代码">' + copyIcon + '</button><div class="code-language-menu" hidden><label class="code-language-search-wrap"><span class="sr-only">搜索代码语言</span><input class="code-language-search" type="search" placeholder="搜索或输入语言…" autocomplete="off"></label><div class="code-language-options" role="listbox" aria-label="代码语言">' + options + '</div><div class="code-language-note">未列出的语言将展示为纯文本</div></div>';
      wrapper.prepend(toolbar);
    });
  }
  function showDocument() {
    const doc = current();
    if (!doc) {
      editor.classList.remove('source-mode');
      editor.classList.add('markdown-body');
      editor.contentEditable = 'false';
      editor.innerHTML = '<div class="empty-document-state">没有打开的文档</div>';
      q('#mode-source').classList.remove('active');
      q('#mode-visual').classList.add('active');
      q('#mode-label').textContent = '所见即所得';
      q('.titlebar strong').textContent = 'Aline';
      document.title = 'Aline';
      q('.editor-area').scrollTop = 0;
      renderTabs(); renderTree(); stats(); remember();
      return;
    }
    const sourceMode = doc.mode === 'source';
    editor.classList.toggle('source-mode',sourceMode);
    editor.classList.toggle('markdown-body',!sourceMode);
    editor.contentEditable = String(!sourceMode);
    if (sourceMode) {
      editor.innerHTML = '<textarea class="source-editor" aria-label="Markdown 源码" spellcheck="false"></textarea>';
      const sourceInput = editor.firstChild;
      sourceInput.value = doc.source;
      const fitSourceInput = () => {
        sourceInput.style.height = 'auto';
        sourceInput.style.height = `${sourceInput.scrollHeight}px`;
      };
      fitSourceInput();
      sourceInput.addEventListener('input', () => {
        fitSourceInput();
        doc.source = sourceInput.value;
        doc.html = renderMarkdown(doc.source);
        doc.dirty = true; sourceChanged = true; renderTabs(); stats(); remember();
      });
      sourceInput.addEventListener('keyup',stats);
      sourceInput.addEventListener('click',stats);
    } else { editor.innerHTML = doc.html || '<p><br></p>'; decorateCodeBlocks(); }
    q('#mode-source').classList.toggle('active',sourceMode);
    q('#mode-visual').classList.toggle('active',!sourceMode);
    q('#mode-label').textContent = sourceMode ? '源码模式' : '所见即所得';
    q('.titlebar strong').textContent = doc.name;
    document.title = `${doc.name} — Aline`;
    q('.editor-area').scrollTop = doc.scroll || 0;
    renderTabs(); renderTree(); stats(); remember();
  }
  function switchDocument(id) {
    if (id === active) return;
    capture(); active = id; sourceChanged = false; showDocument();
  }
  function openDemo(id) {
    capture();
    if (!documents.some(doc => doc.id === id)) {
      const demo = demos.find(doc => doc.id === id);
      if (!demo) return;
      documents.push(structuredClone(demo));
    }
    active = id; sourceChanged = false; showDocument();
  }
  function closeDocument(id) {
    capture();
    const index = documents.findIndex(doc => doc.id === id);
    if (index < 0) return;
    documents.splice(index,1);
    if (!documents.length) { newDocument(); return; }
    if (active === id) active = documents[Math.min(index,documents.length-1)].id;
    showDocument();
  }
  function closeAllDocuments() {
    capture();
    documents = [];
    active = null;
    sourceChanged = false;
    showDocument();
  }
  function newDocument() {
    capture();
    untitledCount += 1;
    const name = `未命名 ${untitledCount}`;
    const doc = {id:`new-${Date.now()}-${untitledCount}`,name,source:'',html:'<p><br></p>',dirty:false,mode:'visual',scroll:0};
    documents.push(doc); active = doc.id; showDocument(); editor.focus();
  }
  function saveSession() {
    capture();
    const doc = current();
    if (!doc) { remember(); return; }
    doc.dirty = false; remember(); renderTabs(); notify('已保存到此浏览器的原型会话');
  }
  function setMode(mode) { capture(); const doc=current(); if(!doc)return; doc.mode = mode; sourceChanged = false; showDocument(); }
  q('#new-doc').onclick = q('#new-tab').onclick = newDocument;
  q('#tab-strip').addEventListener('wheel',event=>{
    const strip=event.currentTarget;
    if (strip.scrollWidth<=strip.clientWidth || Math.abs(event.deltaY)<=Math.abs(event.deltaX)) return;
    strip.scrollLeft += event.deltaY;
    event.preventDefault();
  },{passive:false});
  q('#save-doc').onclick = saveSession;
  q('#save-doc').title = '保存原型会话（⌘S）';
  q('#mode-source').onclick = () => setMode('source');
  q('#mode-visual').onclick = () => setMode('visual');
  qa('.side-switch button').forEach(button => button.onclick = () => {
    sidebarMode = button.dataset.side;
    qa('.side-switch button').forEach(item => item.classList.toggle('selected',item===button));
    renderTree();
  });
  function setCodeLanguage(block, language) {
    block.dataset.language = language;
    block.querySelector('.code-language-value').textContent = languageLabel(language);
    block.querySelectorAll('.code-language-option').forEach(option=>{
      const selected = option.dataset.language === language;
      option.classList.toggle('selected',selected); option.setAttribute('aria-selected',String(selected));
    });
    block.querySelector('.code-language-menu').hidden = true;
    block.querySelector('.code-language-trigger').setAttribute('aria-expanded','false');
    const blockIndex = [...editor.querySelectorAll('.code-block')].indexOf(block);
    let fenceIndex = 0;
    current().source = current().source.replace(new RegExp('^(\\s*\\x60{3,})([\\w+-]*)','gm'),(match,fence)=>fenceIndex++===blockIndex?fence+language:match);
    current().html = editor.innerHTML;
    current().dirty = true; renderTabs(); remember();
  }
  editor.addEventListener('change',event => {
    if (event.target.type !== 'checkbox') return;
    event.target.toggleAttribute('checked',event.target.checked);
    const doc = current(); doc.html = editor.innerHTML; doc.source = htmlToMarkdown(editor); doc.dirty = true; renderTabs(); remember();
  });
  editor.addEventListener('click',async event => {
    const languageTrigger = event.target.closest('.code-language-trigger');
    if (languageTrigger) {
      const block = languageTrigger.closest('.code-block');
      const menu = block.querySelector('.code-language-menu');
      const willOpen = menu.hidden;
      editor.querySelectorAll('.code-language-menu').forEach(item=>{item.hidden=true;item.closest('.code-block')?.querySelector('.code-language-trigger')?.setAttribute('aria-expanded','false');});
      menu.hidden = !willOpen;
      languageTrigger.setAttribute('aria-expanded',String(willOpen));
      if (willOpen) {
        const search=menu.querySelector('.code-language-search'); search.value='';
        menu.querySelectorAll('.code-language-option').forEach(option=>option.hidden=false);
        search.focus();
      }
      return;
    }
    const languageOption = event.target.closest('.code-language-option');
    if (languageOption) { setCodeLanguage(languageOption.closest('.code-block'),languageOption.dataset.language); return; }
    const copyButton = event.target.closest('.copy-code');
    if (copyButton) {
      const code = copyButton.closest('.code-block')?.querySelector('pre code');
      const value = code?.innerText || '';
      try { await navigator.clipboard.writeText(value); showCopied(copyButton); }
      catch (_) {
        const field=document.createElement('textarea');field.value=value;field.style.position='fixed';field.style.opacity='0';document.body.append(field);field.select();document.execCommand('copy');field.remove();showCopied(copyButton);
      }
      return;
    }
    const link = event.target.closest('a');
    if (link && link.getAttribute('href')?.startsWith('#')) {
      event.preventDefault();
      const target = editor.querySelector('[id="' + CSS.escape(link.hash.slice(1)) + '"]');
      target?.scrollIntoView({behavior:'smooth',block:'start'});
    } else if (link && !(event.metaKey || event.ctrlKey)) event.preventDefault();
  });
  editor.addEventListener('input',event=>{
    if (event.target.matches('.code-language-search')) {
      const term=event.target.value.trim().toLowerCase();
      event.target.closest('.code-language-menu').querySelectorAll('.code-language-option').forEach(option=>{option.hidden=!option.dataset.language.includes(term);});
      return;
    }
    if (event.target.tagName === 'TEXTAREA') return;
    editor.querySelectorAll('pre').forEach(refreshLineNumbers);
    const doc = current(); doc.html = editor.innerHTML; doc.source = htmlToMarkdown(editor); doc.dirty = true;
    renderTabs(); stats(); remember();
  });
  editor.addEventListener('keydown',event=>{
    if (event.key==='Escape' && event.target.matches('.code-language-search')) {
      const block=event.target.closest('.code-block'); block.querySelector('.code-language-menu').hidden=true;
      block.querySelector('.code-language-trigger').setAttribute('aria-expanded','false'); block.querySelector('.code-language-trigger').focus();
    }
  });
  document.addEventListener('click',event=>{
    if (event.target.closest('.code-language-menu,.code-language-trigger')) return;
    editor.querySelectorAll('.code-language-menu').forEach(menu=>{
      menu.hidden=true;
      menu.closest('.code-block')?.querySelector('.code-language-trigger')?.setAttribute('aria-expanded','false');
    });
  });
  q('.tab-list').onclick = event => {
    event.stopPropagation();
    const existing = q('.document-tabs-list');
    if (existing) { existing.remove(); return; }
    const menu = document.createElement('div'); menu.className = 'document-tabs-list';
    menu.setAttribute('role','menu');
    const triggerRect = q('.tab-list').getBoundingClientRect();
    const menuTop = triggerRect.bottom + 4;
    menu.style.top = `${menuTop}px`;
    menu.style.right = `${Math.max(8,window.innerWidth-triggerRect.right)}px`;
    menu.style.maxHeight = `${Math.max(120,window.innerHeight-menuTop-8)}px`;
    const items = documents.map(doc => `<button class="tab-list-item${doc.id===active?' active':''}" role="menuitem" data-id="${escape(doc.id)}"><span class="tab-list-name">${escape(doc.name)}</span>${doc.dirty?'<span class="tab-list-dirty" aria-label="未保存">•</span>':''}</button>`).join('');
    menu.innerHTML = `<button class="tab-list-close-all" type="button"${documents.length?'':' disabled'}>全部关闭</button><div class="tab-list-items">${items||'<div class="tab-list-empty">没有打开的标签</div>'}</div>`;
    document.body.append(menu);
    menu.querySelector('.tab-list-close-all').onclick = () => { menu.remove(); closeAllDocuments(); };
    menu.querySelectorAll('.tab-list-item').forEach(button => button.onclick = () => { switchDocument(button.dataset.id); menu.remove(); });
  };
  document.addEventListener('click',event => { if (!event.target.closest('.document-tabs-list,.tab-list')) q('.document-tabs-list')?.remove(); });
  document.addEventListener('keydown',event => {
    if (!(event.metaKey || event.ctrlKey)) return;
    const key = event.key.toLowerCase();
    if (!['s','t','w','/'].includes(key)) return;
    event.preventDefault(); event.stopImmediatePropagation();
    if (key==='s') saveSession();
    if (key==='t') newDocument();
    if (key==='w') closeDocument(active);
    if (key==='/' && current()) setMode(current().mode==='source'?'visual':'source');
  },true);
  const resize = q('.resize-handle');
  resize.addEventListener('pointerdown',event => {
    event.preventDefault(); resize.setPointerCapture(event.pointerId);
    const move = pointer => {
      const width = Math.max(200,Math.min(420,pointer.clientX));
      q('#app').style.gridTemplateColumns = `${width}px minmax(0,1fr)`;
      q('#sidebar').style.width = `${width}px`;
    };
    const finish = () => { resize.removeEventListener('pointermove',move); resize.removeEventListener('pointerup',finish); };
    resize.addEventListener('pointermove',move); resize.addEventListener('pointerup',finish,{once:true});
  });
  q('#sidebar-toggle').onclick = () => {
    const app = q('#app'); app.classList.toggle('sidebar-hidden');
    app.style.gridTemplateColumns = app.classList.contains('sidebar-hidden') ? '0 minmax(0,1fr)' : `${q('#sidebar').style.width || '250px'} minmax(0,1fr)`;
  };
  /* 设置控件按分类保存，切换分类后恢复；仅在浏览器内保留。 */
  const settingsState = {};
  const defaultPages = {};
  const originalPage = window.setPage;
  function rememberSettings() {
    const page = q('#settings-current-page').dataset.page || q('#settings-current-page').textContent;
    settingsState[page] = qa('#settings-content input,#settings-content select,#settings-content .toggle').map(el => el.matches('.toggle') ? el.classList.contains('active') : el.value);
  }
  function settingsPage(page) {
    originalPage(page);
    defaultPages[page] = q('#settings-content').innerHTML;
    const controls = qa('#settings-content input,#settings-content select,#settings-content .toggle');
    controls.forEach((control,index) => {
      const value = settingsState[page]?.[index];
      if (value !== undefined) {
        if (control.matches('.toggle')) { control.classList.toggle('active',value); control.setAttribute('aria-checked',String(value)); }
        else control.value = value;
      }
      control.addEventListener('change',rememberSettings);
      if (control.matches('.toggle')) control.addEventListener('click',rememberSettings);
    });
    qa('#settings-content .settings-reset').forEach(button => button.onclick = () => {
      const section = button.closest('.settings-group');
      const defaults = document.createElement('div'); defaults.innerHTML = defaultPages[page] || '';
      section.querySelectorAll('input,select,.toggle').forEach(control => {
        const i = controls.indexOf(control);
        const value = defaults.querySelectorAll('input,select,.toggle')[i];
        if (!value) { delete settingsState[page]; settingsPage(page); return; }
        if (control.matches('.toggle')) { control.classList.toggle('active',value.classList.contains('active')); control.setAttribute('aria-checked',String(value.classList.contains('active'))); }
        else control.value = value.value;
      });
      rememberSettings(); notify('本组设置已恢复默认');
    });
  }
  window.setPage = settingsPage;
  qa('#settings-nav button').forEach(button => button.onclick = () => { rememberSettings(); settingsPage(button.dataset.page); });
  q('#settings-search').oninput = event => {
    const term = event.target.value.trim().toLowerCase();
    qa('#settings-nav button').forEach(button => { button.hidden = !!term && !button.textContent.toLowerCase().includes(term); });
    if (term) {
      const match = qa('#settings-nav button').find(button => !button.hidden);
      if (match) settingsPage(match.dataset.page);
    }
  };
  function htmlToMarkdown(root) {
    root = root.cloneNode(true);
    root.querySelectorAll('.code-toolbar,.code-gutter').forEach(node=>node.remove());
    root.querySelectorAll('.code-block').forEach(block=>{const pre=block.querySelector('pre');if(pre)pre.dataset.sourceLanguage=block.dataset.language||'text';});
    const walk = node => {
      if (node.nodeType === Node.TEXT_NODE) return node.textContent;
      if (node.nodeType !== Node.ELEMENT_NODE) return '';
      const body = [...node.childNodes].map(walk).join('');
      const tag = node.tagName.toLowerCase();
      if (/^h[1-6]$/.test(tag)) return '#'.repeat(Number(tag[1])) + ' ' + body + '\n\n';
      if (tag==='strong'||tag==='b') return '**' + body + '**';
      if (tag==='em'||tag==='i') return '*' + body + '*';
      if (tag==='del'||tag==='s') return '~~' + body + '~~';
      if (tag==='pre') return '\n' + String.fromCharCode(96).repeat(3) + (node.dataset.sourceLanguage||'') + '\n' + (node.querySelector('code')?.textContent||node.textContent) + '\n' + String.fromCharCode(96).repeat(3) + '\n\n';
      if (tag==='code') { const tick=String.fromCharCode(96); return tick + body + tick; }
      if (tag==='a') return '[' + body + '](' + (node.getAttribute('href')||'') + ')';
      if (tag==='img') return '![' + (node.getAttribute('alt')||'') + '](' + (node.getAttribute('src')||'') + ')';
      if (tag==='svg') return '\n' + node.outerHTML + '\n';
      if (tag==='math') return '\n' + node.textContent + '\n';
      if (tag==='table') {
        const rows = [...node.querySelectorAll('tr')].map(row=>[...row.children].map(cell=>cell.textContent.replace(/\|/g,'\\|').trim()));
        return '\n' + rows.map((cells,index)=>'| ' + cells.join(' | ') + ' |' + (index===0?'\n| '+cells.map(()=>'---').join(' | ')+' |':'')).join('\n') + '\n\n';
      }
      if (tag==='br') return '\n';
      if (tag==='li') return '- ' + body + '\n';
      if (tag==='blockquote') return body.trim().split('\n').map(line=>'> '+line).join('\n') + '\n\n';
      if (tag==='input' && node.type==='checkbox') return '[' + (node.checked?'x':' ') + '] ';
      if (tag==='p'||tag==='div') return body + '\n\n';
      return body;
    };
    return [...root.childNodes].map(walk).join('').trim();
  }
  /* 用户修改源码后的轻量预览，先转义 HTML，避免将输入当作页面脚本执行。 */
  function renderMarkdown(source) {
    const tick=String.fromCharCode(96), fence=tick.repeat(3);
    const inline = raw => escape(raw).replace(new RegExp(tick+'([^'+tick+']+)'+tick,'g'),'<code>$1</code>').replace(/\*\*([^*]+)\*\*/g,'<strong>$1</strong>').replace(/~~([^~]+)~~/g,'<del>$1</del>').replace(/\*([^*]+)\*/g,'<em>$1</em>');
    const lines = source.split('\n'); let output = ''; let fenced = false; let code = [];
    for (const line of lines) {
      if (line.startsWith(fence)) {
        if (fenced) { output += '<pre><code>' + escape(code.join('\n')) + '</code></pre>'; code=[]; }
        fenced=!fenced; continue;
      }
      if (fenced) { code.push(line); continue; }
      const heading = line.match(/^(#{1,6})\s+(.*)$/);
      if (heading) { output += '<h'+heading[1].length+'>'+inline(heading[2])+'</h'+heading[1].length+'>'; continue; }
      if (/^\s*([-*_])(?:\s*\1){2,}\s*$/.test(line)) { output+='<hr>'; continue; }
      const task = line.match(/^\s*[-*]\s+\[([ xX])\]\s+(.*)$/);
      if (task) { output+='<ul class="contains-task-list"><li><input type="checkbox"'+(task[1].toLowerCase()==='x'?' checked':'')+'> '+inline(task[2])+'</li></ul>'; continue; }
      const list = line.match(/^\s*([-*]|\d+[.)])\s+(.*)$/);
      if (list) { const type=/\d/.test(list[1])?'ol':'ul'; output+='<'+type+'><li>'+inline(list[2])+'</li></'+type+'>'; continue; }
      if (/^>\s?/.test(line)) { output+='<blockquote><p>'+inline(line.replace(/^>\s?/,''))+'</p></blockquote>'; continue; }
      if (line.trim()) output+='<p>'+inline(line)+'</p>';
    }
    if (fenced) output+='<pre><code>'+escape(code.join('\n'))+'</code></pre>';
    return output || '<p><br></p>';
  }

  function insertMarkdown(before, after='') {
    const input = editor.querySelector('textarea');
    if (!input) {
      editor.focus();
      const selection = window.getSelection();
      const text = selection && editor.contains(selection.anchorNode) ? selection.toString() : '';
      document.execCommand('insertHTML',false,renderMarkdown(before + text + after));
      editor.dispatchEvent(new Event('input',{bubbles:true}));
      return;
    }
    const start = input.selectionStart, end = input.selectionEnd;
    const selected = input.value.slice(start,end);
    input.setRangeText(before+selected+after,start,end,'end');
    input.focus();
    input.setSelectionRange(start+before.length,start+before.length+selected.length);
    input.dispatchEvent(new Event('input',{bubbles:true}));
  }
  const tick=String.fromCharCode(96);
  const commands = {
    '粗体':['bold','**','**'], '斜体':['italic','*','*'], '删除线':['strikeThrough','~~','~~'],
    '行内代码':[null,tick,tick], '标题 1':['formatBlock','\n# ','','h1'], '标题 2':['formatBlock','\n## ','','h2'],
    '引用':['formatBlock','\n> ','','blockquote'], '无序列表':['insertUnorderedList','\n- ',''],
    '有序列表':['insertOrderedList','\n1. ',''], '分隔线':['insertHorizontalRule','\n\n---\n\n',''],
    '代码块':[null,'\n'+tick.repeat(3)+'\n','\n'+tick.repeat(3)+'\n'], '链接':[null,'[','](https://github.com)'],
    '插入图片':[null,'![图片说明](./assets/showcase-landscape.svg)','']
  };
  qa('#toolbar button[title]').forEach(button => {
    const command = commands[button.title];
    if (!command) return;
    button.addEventListener('pointerdown',event => event.preventDefault());
    button.onclick = () => {
      if (!current()) return;
      if (current().mode==='source' || !command[0]) { insertMarkdown(command[1],command[2]); return; }
      editor.focus(); document.execCommand(command[0],false,command[3] || null);
      editor.dispatchEvent(new Event('input',{bubbles:true}));
    };
  });
  const showcaseButton = q('#export-btn');
  if (showcaseButton && showcaseButton.title==='Markdown 示例') showcaseButton.onclick = () => openDemo(demos[0].id);
  window.alinePrototype = {openDocument:openDemo,insertMarkdown,saveCurrent:saveSession,getCurrent:current};
  window.alineWorkspace = {openDemo,newDocument,saveSession,getCurrent:current};
  showDocument();
})();
