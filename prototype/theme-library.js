/* 内置主题目录、预览图片与 ZIP 主题包的原型交互。 */
(() => {
  'use strict';
  const $ = selector => document.querySelector(selector);
  const modes = [['light','白天'],['dark','夜间'],['sepia','护眼']];
  const builtinIds = ['github','notion','vue','feishu'];
  const themeDescriptions = {
    github:'沿用 GitHub Markdown 的清晰标题层级与蓝色链接，表格和代码块用轻量灰底分组。素白画布搭配中性边框，适合长文与技术文档阅读。',
    notion:'以柔和灰白和宽松留白突出正文，标题分层简洁，代码块和表格保持低对比表面。整体减少界面强调，适合笔记、知识整理和长时间阅读。',
    vue:'以 Vue 绿作为链接和交互强调色，白色画布搭配清晰标题层级与深色代码区。结构轻快、对比清楚，适合组件文档和开发指南。',
    feishu:'用飞书蓝做链接与交互强调，浅灰侧栏和细边框分出内容区域。表格、代码采用克制底色，适合协作说明、流程文档和日常笔记。'
  };
  const schema = {
    markdown:{
      colors:['body','muted','subtle','link','selection','caret','error','errorBackground'],
      typography:['bodyFamilies','headingFamilies','codeFamilies','bodySize','bodyWeight','lineHeight','bodyAlign'],
      headings:{each:['families','size','weight','lineHeight','color','align','marginBefore','marginAfter']},
      spacing:['paragraphAfter','listItemGap','blockBefore','blockAfter','quotePadding'],
      inline:{strong:['weight','color'],emphasis:['italic','color'],strike:['enabled','color'],link:['color','underline'],codeSpan:['background','color','radius']},
      list:['color','background','border','markerColor'],blockquote:['color','background','border','lineColor'],task:['color','background','border','accent'],rule:['color','background','border'],image:['color','background','border','captionColor'],
      callout:{each:['color','background','border']},formula:['color','background','border'],diagram:['color','background','border'],
      codeBlock:['background','color','border','radius','paddingX','paddingY','fontSize','lineHeight','lineNumberColor','lineNumberBackground'],
      table:['headerBackground','headerColor','headerWeight','cellBackground','alternateBackground','zebra','border','cellPaddingX','cellPaddingY','rowLineHeight']
    },
    highlight:{syntax:['comment','keyword','string','number','function','type','operator','punctuation','tag','attribute','variable','macro','inserted','deleted'],codeBlock:['background','color','border','lineNumberColor','lineNumberBackground']},
      application:{typography:['uiFamilies','uiSize','uiWeight'],colors:['window','editor','surface','sidebar','toolbar','tabBar','statusBar','menu','input','text','muted','accent','border','selection','hover','focus','danger'],window:['backgroundOpacity'],icons:['style','strokeWidth','default','muted','accent','active','disabled','danger'],fileTree:{each:['style','color']}}
  };
  let catalog = [], refreshSequence = 0, activePreview = null, appliedTokens = new Set();
  let highlightThemeSetting = 'follow-app', highlightModeSetting = 'follow-app', highlightSettingsLoaded = false;
  const escapeHtml = value => String(value).replace(/[&<>"']/g, char => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[char]));
  const report = (message, error=false) => { const status=$('#theme-library-status');if(status){status.textContent=message;status.classList.toggle('error',error);} };
  const safePath = value => typeof value==='string' && value.length>0 && !value.startsWith('/') && !/^[a-z]:/i.test(value) && !value.includes('\\') && !value.split('/').includes('..') && !value.split('/').includes('');
  const merge = (base, override) => {
    if(!override || typeof override!=='object' || Array.isArray(override)) return override===undefined ? structuredClone(base) : override;
    const result=base&&typeof base==='object'&&!Array.isArray(base)?structuredClone(base):{};
    Object.entries(override).forEach(([key,value])=>{result[key]=value&&typeof value==='object'&&!Array.isArray(value)?merge(result[key],value):value;});
    return result;
  };
  const modeConfig = (config,mode) => merge(config.common||{},config.modes?.[mode]||{});
  function validateTree(value,shape,path){
    if(!value||typeof value!=='object'||Array.isArray(value))throw new Error(`${path} 必须是 JSON 对象`);
    for(const [key,child] of Object.entries(value)){
      if(!(key in shape))throw new Error(`${path}.${key} 不是支持的主题字段`);
      const rule=shape[key];
      if(Array.isArray(rule)){
        if(child&&typeof child==='object'&&!Array.isArray(child)){for(const sub of Object.keys(child))if(!rule.includes(sub))throw new Error(`${path}.${key}.${sub} 不是支持的主题字段`);}
        else if(!(typeof child==='string'||typeof child==='number'||typeof child==='boolean'||Array.isArray(child)))throw new Error(`${path}.${key} 类型无效`);
        continue;
      }
      if(rule&&typeof rule==='object'){
        if(rule.each){for(const [sub,entry] of Object.entries(child||{})){if(!/^(h[1-6]|note|tip|important|warning|caution|folderClosed|folderOpen|markdownFile|imageFile|otherFile)$/.test(sub))throw new Error(`${path}.${key}.${sub} 字段无效`);validateTree(entry,Object.fromEntries(rule.each.map(x=>[x,[]])),`${path}.${key}.${sub}`);}}
        else for(const [sub,entry] of Object.entries(child||{})){if(!(sub in rule))throw new Error(`${path}.${key}.${sub} 不是支持的主题字段`);const subRule=rule[sub];if(Array.isArray(subRule)){if(entry&&typeof entry==='object'&&!Array.isArray(entry)){for(const leaf of Object.keys(entry))if(!subRule.includes(leaf))throw new Error(`${path}.${key}.${sub}.${leaf} 不是支持的主题字段`);}else if(!(typeof entry==='number'||typeof entry==='boolean'||typeof entry==='string'||Array.isArray(entry)))throw new Error(`${path}.${key}.${sub} 类型无效`);}else validateTree(entry,subRule,`${path}.${key}.${sub}`);}
      }
    }
  }
  function checkValues(value,path='theme'){
    for(const [key,item] of Object.entries(value||{})){
      const at=`${path}.${key}`;
      if(item===null)throw new Error(`${at} 不接受 null`);
      if(key==='align'&&!['left','center','right','justify'].includes(item))throw new Error(`${at} 对齐值无效`);
      if(key==='style'&&!['outline','filled'].includes(item))throw new Error(`${at} 图标样式只支持 outline 或 filled`);
      if(['zebra','underline','italic','enabled'].includes(key)&&typeof item!=='boolean')throw new Error(`${at} 必须是布尔值`);
      const colorKey=!/Opacity$/i.test(key)&&(key==='body'||/color|background|border|accent|muted|subtle|link|selection|caret|error|window|sidebar|toolbar|tabBar|statusBar|menu|input|text|focus|danger|default|disabled|active|folder|File|lineNumber|comment|keyword|string|number|function|type|operator|punctuation|tag|attribute|variable|macro|inserted|deleted/i.test(key));
      if(colorKey&&typeof item!=='string'&&!(item&&typeof item==='object'&&!Array.isArray(item)))throw new Error(`${at} 颜色必须使用 #RRGGBB`);
      if(typeof item==='string'&&colorKey&&!/^#[0-9a-f]{6}$/i.test(item))throw new Error(`${at} 颜色必须使用 #RRGGBB`);
      if(/Families$/.test(key)&&(!Array.isArray(item)||!item.length||item.some(font=>typeof font!=='string'||!font.trim()||font.length>128)))throw new Error(`${at} 必须是有效的字体族列表`);
      if(['bodySize','uiSize','bodyWeight','uiWeight','size','weight','lineHeight','paragraphAfter','listItemGap','blockBefore','blockAfter','quotePadding','radius','backgroundOpacity','strokeWidth','cellPaddingX','cellPaddingY','rowLineHeight','fontSize','paddingX','paddingY','headerWeight'].includes(key)&&typeof item!=='number')throw new Error(`${at} 必须是数值`);
      if(typeof item==='number'){
        if(!Number.isFinite(item))throw new Error(`${at} 必须是有限数值`);
        if(/uiSize/i.test(key)&&(item<11||item>20))throw new Error(`${at} 范围必须为 11–20`);
        if(/(?:^size$|fontSize)/i.test(key)&&(item<10||item>72))throw new Error(`${at} 范围必须为 10–72`);
        if(/weight/i.test(key)&&(item<100||item>900||item%100!==0))throw new Error(`${at} 字重必须为 100–900 的百位值`);
        if(/lineHeight/i.test(key)&&(item<1||item>3))throw new Error(`${at} 行高必须为 1–3`);
        if(/Opacity/i.test(key)&&(item<0||item>1))throw new Error(`${at} 透明度必须为 0–1`);
        if(/strokeWidth/i.test(key)&&(item<.75||item>3))throw new Error(`${at} 描边宽度必须为 0.75–3`);
        if(/radius/i.test(key)&&(item<0||item>16))throw new Error(`${at} 圆角必须为 0–16`);
        if(/paragraphAfter|listItemGap|blockBefore|blockAfter|quotePadding|paddingX|paddingY|cellPadding/i.test(key)&&(item<0||item>128))throw new Error(`${at} 间距必须为 0–128`);
        if(/marginBefore|marginAfter/i.test(key)&&(item<0||item>128))throw new Error(`${at} 标题间距必须为 0–128`);
      }
      if(typeof item==='object'&&!Array.isArray(item))checkValues(item,at);
    }
  }
  function validateConfig(config,type){
    if(!config||typeof config!=='object'||Array.isArray(config))throw new Error(`${type}.json 必须是对象`);
    for(const key of Object.keys(config))if(!['common','modes'].includes(key))throw new Error(`${type}.json 不支持 ${key}`);
    if(config.common)validateTree(config.common,schema[type],`${type}.common`);
    if(config.modes){for(const [mode,overrides] of Object.entries(config.modes)){if(!modes.some(([id])=>id===mode))throw new Error(`${type}.json 模式无效：${mode}`);validateTree(overrides,schema[type],`${type}.modes.${mode}`);}}
    checkValues(config,`${type}.json`);
    return config;
  }
  function validateManifest(theme,id){
    if(theme.schemaVersion!==3||theme.id!==id||!/^[a-z0-9][a-z0-9-]{0,47}$/.test(theme.id||'')||typeof theme.name!=='string'||theme.name.length<1||theme.name.length>48||typeof theme.version!=='string'||!theme.version.trim())throw new Error(`主题清单无效：${id}`);
    for(const key of Object.keys(theme))if(!['schemaVersion','id','name','version','author','inherits','files','previews','fonts'].includes(key))throw new Error(`${id} 的 theme.json 不支持字段 ${key}`);
    if(theme.inherits!==undefined&&theme.inherits!=='github')throw new Error(`${id} 当前只支持继承 GitHub 默认主题`);
    if(!theme.files||!safePath(theme.files.markdown)||!safePath(theme.files.application)||!safePath(theme.files.highlight))throw new Error(`${id} 必须声明安全的 markdown.json、application.json 与 highlight.json 路径`);
    const paths=new Set();
    for(const [mode] of modes){const path=theme.previews?.[mode];if(!safePath(path)||paths.has(path))throw new Error(`${id} 必须提供三种模式各不相同的安全预览路径`);paths.add(path);}
    if(theme.fonts!==undefined&&(!Array.isArray(theme.fonts)||theme.fonts.length>8))throw new Error(`${id} 字体列表最多包含 8 个字体`);
    if(Object.keys(theme.files).some(key=>!['markdown','application','highlight'].includes(key))||Object.keys(theme.previews).some(key=>!modes.some(([mode])=>mode===key)))throw new Error(`${id} 的 files 或 previews 包含未知字段`);
    for(const font of theme.fonts||[]){if(Object.keys(font).some(key=>!['path','family','license','licenseFile'].includes(key))||!safePath(font.path)||!safePath(font.licenseFile)||!font.family||font.family.length>128||!font.license||font.license.length>96)throw new Error(`${id} 的字体必须声明安全路径、家族名、许可证和许可证文件`);}
    return theme;
  }
  function openThemeDb(){return new Promise((resolve,reject)=>{const request=indexedDB.open('aline-theme-prototype-v2',1);request.onupgradeneeded=()=>request.result.createObjectStore('themes',{keyPath:'id'});request.onsuccess=()=>resolve(request.result);request.onerror=()=>reject(request.error);});}
  async function withStore(mode,work){const db=await openThemeDb();try{return await new Promise((resolve,reject)=>{const tx=db.transaction('themes',mode),store=tx.objectStore('themes');let result;work(store,value=>result=value);tx.oncomplete=()=>resolve(result);tx.onerror=()=>reject(tx.error);tx.onabort=()=>reject(tx.error);});}finally{db.close();}}
  const readCustomThemes=()=>withStore('readonly',(store,set)=>{const req=store.getAll();req.onsuccess=()=>set(req.result||[]);});
  const saveCustomTheme=theme=>withStore('readwrite',store=>store.put(theme));
  function migrateSchema2Theme(theme){
    if(theme?.schemaVersion!==2||!theme.configs?.markdown)return theme;
    const markdown=structuredClone(theme.configs.markdown),highlight={common:{},modes:{}};
    if(markdown.common?.syntax){highlight.common.syntax=markdown.common.syntax;delete markdown.common.syntax;}
    for(const [mode,values] of Object.entries(markdown.modes||{}))if(values.syntax){highlight.modes[mode]={syntax:values.syntax};delete values.syntax;}
    return {...theme,schemaVersion:3,files:{...theme.files,highlight:theme.files?.highlight||'highlight.json'},configs:{...theme.configs,markdown,highlight}};
  }
  async function migrateOldThemes(){
    let old=[];try{old=JSON.parse(localStorage.getItem('aline.customThemes')||'[]');}catch{}
    const current=await readCustomThemes(),ids=new Set(current.map(item=>item.id));
    for(const item of current){const upgraded=migrateSchema2Theme(item);if(upgraded!==item&&upgraded.schemaVersion===3)await saveCustomTheme(upgraded);}
    if(!Array.isArray(old)||!old.length)return;
    let allMigrated=true;
    for(const item of old){const upgraded=migrateSchema2Theme(item);if(upgraded?.schemaVersion===3&&!ids.has(upgraded.id)){await saveCustomTheme(upgraded);ids.add(upgraded.id);}else if(upgraded?.schemaVersion!==3)allMigrated=false;}
    if(allMigrated)localStorage.removeItem('aline.customThemes');
  }
  const accentFor=theme=>theme.resolved?.application?.colors?.accent||'#0969da';
  function renderCards(){
    const gallery=$('#theme-gallery');if(!gallery)return;
    const selected=document.documentElement.dataset.palette||'github',currentMode=document.documentElement.dataset.theme||'light';
    gallery.innerHTML=catalog.map(theme=>{
      const id=escapeHtml(theme.id),name=escapeHtml(theme.name),accent=escapeHtml(accentFor(theme));
      const src=theme.previewData?.[currentMode]||`./themes/${encodeURIComponent(theme.id)}/${theme.previews[currentMode].split('/').map(encodeURIComponent).join('/')}`;
      const label=modes.find(([mode])=>mode===currentMode)?.[1]||'白天',description=theme.description||themeDescriptions[theme.id]||`${theme.name}主题为 Markdown 阅读与编辑提供一套协调的界面配色。`;
      const origin=theme.builtin?'<small class="theme-origin">内置</small>':'<small class="theme-origin imported">已导入</small>';
      return `<article class="theme-family-card${theme.id===selected?' selected':''}" data-palette="${id}" style="--theme-card-accent:${accent}"><div class="theme-family-heading"><button type="button" class="theme-family-select" data-palette="${id}" aria-pressed="${theme.id===selected}"><span class="theme-accent-swatch" aria-hidden="true"></span><strong>${name}</strong>${theme.id==='github'&&theme.builtin?'<small>默认</small>':''}${origin}</button></div><div class="theme-preview-row"><button type="button" class="theme-preview-open" data-theme-id="${id}" data-preview-mode="${currentMode}" aria-label="预览 ${name} 主题，当前为${label}模式"><img src="${escapeHtml(src)}" alt="${name} ${label}模式界面预览"><span class="theme-preview-overlay"><span>预览</span></span></button><p class="theme-description" tabindex="0" title="${escapeHtml(description)}" data-full="${escapeHtml(description)}" aria-label="${escapeHtml(description)}">${escapeHtml(description)}</p></div></article>`;
    }).join('');
    syncHighlightSettings();
  }
  function syncHighlightSettings(){
    const themeSelect=$('#highlight-theme-select'),modeSelect=$('#highlight-mode-select');
    if(!themeSelect&&!modeSelect)return;
    if(themeSelect){
      const current=highlightThemeSetting;
      themeSelect.innerHTML=`<option value="follow-app">随应用切换</option>${catalog.map(theme=>`<option value="${escapeHtml(theme.id)}">${escapeHtml(theme.name)}</option>`).join('')}`;
      themeSelect.value=current==='follow-app'||catalog.some(theme=>theme.id===current)?current:'follow-app';
      themeSelect.onchange=()=>setHighlightSettings(themeSelect.value,highlightModeSetting);
    }
    if(modeSelect){
      modeSelect.value=highlightModeSetting;
      modeSelect.onchange=()=>setHighlightSettings(highlightThemeSetting,modeSelect.value);
    }
    const themeId=highlightThemeSetting==='follow-app'?(document.documentElement.dataset.palette||'github'):highlightThemeSetting;
    const mode=highlightModeSetting==='follow-app'?(document.documentElement.dataset.theme||'light'):highlightModeSetting;
    const themeName=catalog.find(theme=>theme.id===themeId)?.name||'GitHub';
    const modeName=modes.find(([id])=>id===mode)?.[1]||'白天';
    const description=$('#highlight-settings-description');
    if(description)description.textContent=`当前代码高亮：${themeName} · ${modeName}。可在白天应用界面中单独使用暗色代码块与语法配色。`;
  }
  function setHighlightSettings(themeId,mode){
    highlightThemeSetting=themeId==='follow-app'||catalog.some(theme=>theme.id===themeId)?themeId:'follow-app';
    highlightModeSetting=mode==='follow-app'||modes.some(([id])=>id===mode)?mode:'follow-app';
    try{localStorage.setItem('aline.highlightTheme',highlightThemeSetting);localStorage.setItem('aline.highlightMode',highlightModeSetting);}catch{}
    applyTheme(document.documentElement.dataset.palette||'github',document.documentElement.dataset.theme||'light');
  }
  window.syncHighlightSettings=syncHighlightSettings;
  function syncCards(){const selected=document.documentElement.dataset.palette||'github';document.querySelectorAll('.theme-family-card').forEach(card=>{const active=card.dataset.palette===selected;card.classList.toggle('selected',active);card.querySelector('.theme-family-select')?.setAttribute('aria-pressed',String(active));});}
  async function readTheme(base,id,builtin){
    const manifestPath=base?`${base}/theme.json`:`./themes/${id}/theme.json`;
    const root=base||`./themes/${id}`;
    const response=await fetch(`${manifestPath}?refresh=${Date.now()}`,{cache:'no-store'});if(!response.ok)throw new Error(`主题包清单读取失败：${id}`);
    const manifest=validateManifest(await response.json(),id);
    const fetchJson=async path=>{const result=await fetch(`${root}/${path.split('/').map(encodeURIComponent).join('/')}?refresh=${Date.now()}`,{cache:'no-store'});if(!result.ok)throw new Error(`${id} 缺少 ${path}`);return result.json();};
    const [markdown,application,highlight]=await Promise.all([fetchJson(manifest.files.markdown),fetchJson(manifest.files.application),fetchJson(manifest.files.highlight)]);
    validateConfig(markdown,'markdown');validateConfig(application,'application');validateConfig(highlight,'highlight');
    const previewData={};
    for(const [mode] of modes){const relative=manifest.previews[mode],imageResult=await fetch(`${root}/${relative.split('/').map(encodeURIComponent).join('/')}?refresh=${Date.now()}`,{cache:'no-store'});if(!imageResult.ok)throw new Error(`${id} 缺少 ${mode} 预览图`);const image=new Uint8Array(await imageResult.arrayBuffer());if(image.length>1024*1024)throw new Error(`${id} 的预览图超过 1 MB`);await decodePreview(image,relative.split('.').pop().toLowerCase());}
    const github=id==='github'?null:catalog.find(item=>item.id==='github')?.configs;
    return {...manifest,builtin,configs:{markdown,application,highlight},previewData, resolved:{markdown:{},application:{},highlight:{}},basePath:root};
  }
  function applyTheme(id,mode){
    const theme=catalog.find(item=>item.id===id)||catalog.find(item=>item.id==='github');if(!theme)return;
    const root=document.documentElement;
    for(const token of appliedTokens)root.style.removeProperty(token);appliedTokens.clear();
    const fallback=catalog.find(item=>item.id==='github')||theme;
    const highlightId=highlightThemeSetting==='follow-app'?id:highlightThemeSetting;
    const resolvedHighlightMode=highlightModeSetting==='follow-app'?mode:highlightModeSetting;
    const highlightTheme=catalog.find(item=>item.id===highlightId)||fallback;
    const resolved={};
    for(const type of ['markdown','application']){
      const base=modeConfig(fallback.configs?.[type]||{},mode);
      resolved[type]=merge(base,modeConfig(theme.configs?.[type]||{},mode));
    }
    resolved.highlight=merge(modeConfig(fallback.configs?.highlight||{},resolvedHighlightMode),modeConfig(highlightTheme.configs?.highlight||{},resolvedHighlightMode));
    theme.resolved=resolved;
    const css={};
    const app=resolved.application,md=resolved.markdown,colors=app.colors||{},mcolors=md.colors||{},uiType=app.typography||{},typography=md.typography||{};
    Object.entries({window:'--app-window',editor:'--app-editor',surface:'--app-surface',sidebar:'--app-sidebar',toolbar:'--app-toolbar',tabBar:'--app-tabbar',statusBar:'--app-statusbar',menu:'--app-menu',input:'--app-input',text:'--app-text',muted:'--app-muted',accent:'--accent',border:'--app-border',selection:'--app-selection',hover:'--app-hover',focus:'--app-focus',danger:'--app-danger'}).forEach(([key,varName])=>{if(colors[key])css[varName]=colors[key];});
    for(const [key,alias] of [['surface','--surface'],['text','--fg'],['muted','--muted'],['border','--border'],['hover','--hover'],['selection','--accent-soft']])if(colors[key])css[alias]=colors[key];
    Object.entries({body:'--md-body-color',muted:'--md-muted',subtle:'--md-subtle',link:'--md-link',selection:'--md-selection',caret:'--md-caret',error:'--md-error',errorBackground:'--md-error-bg'}).forEach(([key,varName])=>{if(mcolors[key])css[varName]=mcolors[key];});
    const family=value=>Array.isArray(value)?value.map(font=>/^(system-ui|sans-serif|serif|monospace|inherit|-apple-system|BlinkMacSystemFont|Segoe UI|SFMono-Regular|Consolas)$/i.test(font)?font:`"${font.replace(/["\\]/g,'')}"`).join(', '):undefined;
    if(family(uiType.uiFamilies))css['--app-font']=family(uiType.uiFamilies);if(uiType.uiSize)css['--app-font-size']=`${uiType.uiSize}px`;if(uiType.uiWeight)css['--app-font-weight']=uiType.uiWeight;
    if(family(typography.bodyFamilies))css['--md-body-font']=family(typography.bodyFamilies);if(family(typography.headingFamilies))css['--md-heading-font']=family(typography.headingFamilies);if(family(typography.codeFamilies))css['--md-code-font']=family(typography.codeFamilies);
    if(typography.bodySize)css['--md-body-size']=`${typography.bodySize}px`;if(typography.bodyWeight)css['--md-body-weight']=typography.bodyWeight;if(typography.lineHeight)css['--md-body-line-height']=typography.lineHeight;if(typography.bodyAlign)css['--md-body-align']=typography.bodyAlign;
    Object.entries(md.headings||{}).forEach(([heading,values])=>{if(family(values.families))css[`--md-${heading}-font`]=family(values.families);Object.entries({size:'size',weight:'weight',lineHeight:'line-height',color:'color',align:'align',marginBefore:'margin-before',marginAfter:'margin-after'}).forEach(([key,property])=>{if(values[key]!==undefined)css[`--md-${heading}-${property}`]=['size','margin-before','margin-after'].includes(property)?`${values[key]}px`:values[key];});});
    const spacing=md.spacing||{};Object.entries({paragraphAfter:'--md-paragraph-gap',listItemGap:'--md-list-gap',blockBefore:'--md-block-before',blockAfter:'--md-block-after',quotePadding:'--md-quote-padding'}).forEach(([key,varName])=>{if(spacing[key]!==undefined)css[varName]=`${spacing[key]}px`;});
    const code=md.codeBlock||{};Object.entries({background:'--md-code-bg',color:'--md-code-color',border:'--md-code-border',lineNumberColor:'--md-line-number',lineNumberBackground:'--md-line-number-bg'}).forEach(([key,varName])=>{if(code[key])css[varName]=code[key];});for(const key of ['radius','paddingX','paddingY','fontSize','lineHeight'])if(code[key]!==undefined)css[`--md-code-${key.replace(/[A-Z]/g,m=>`-${m.toLowerCase()}`)}`]=['radius','paddingX','paddingY','fontSize'].includes(key)?`${code[key]}px`:code[key];
    const table=md.table||{};Object.entries({headerBackground:'--md-table-head-bg',headerColor:'--md-table-head-color',cellBackground:'--md-table-cell-bg',alternateBackground:'--md-table-alt-bg',border:'--md-table-border'}).forEach(([key,varName])=>{if(table[key])css[varName]=table[key];});if(table.headerWeight)css['--md-table-head-weight']=table.headerWeight;if(table.zebra!==undefined)css['--md-table-zebra']=table.zebra?'1':'0';for(const key of ['cellPaddingX','cellPaddingY'])if(table[key]!==undefined)css[`--md-table-${key.replace(/[A-Z]/g,m=>`-${m.toLowerCase()}`)}`]=`${table[key]}px`;if(table.rowLineHeight)css['--md-table-line-height']=table.rowLineHeight;
    const syntax=resolved.highlight.syntax||{};Object.entries(syntax).forEach(([key,value])=>css[`--md-syntax-${key.replace(/[A-Z]/g,m=>`-${m.toLowerCase()}`)}`]=value);
    const highlightedCode=resolved.highlight.codeBlock||{};Object.entries({background:'--md-code-bg',color:'--md-code-color',border:'--md-code-border',lineNumberColor:'--md-line-number',lineNumberBackground:'--md-line-number-bg'}).forEach(([key,token])=>{if(highlightedCode[key])css[token]=highlightedCode[key];});
    const inline=md.inline||{};if(inline.strong?.weight)css['--md-strong-weight']=inline.strong.weight;if(inline.strong?.color)css['--md-strong-color']=inline.strong.color;if(inline.emphasis?.color)css['--md-emphasis-color']=inline.emphasis.color;if(inline.emphasis?.italic!==undefined)css['--md-emphasis-style']=inline.emphasis.italic?'italic':'normal';if(inline.strike?.enabled!==undefined)css['--md-strike-decoration']=inline.strike.enabled?'line-through':'none';if(inline.strike?.color)css['--md-strike-color']=inline.strike.color;if(inline.link?.underline!==undefined)css['--md-link-decoration']=inline.link.underline?'underline':'none';if(inline.codeSpan?.background)css['--md-inline-code-bg']=inline.codeSpan.background;if(inline.codeSpan?.color)css['--md-inline-code-color']=inline.codeSpan.color;if(inline.codeSpan?.radius!==undefined)css['--md-inline-code-radius']=`${inline.codeSpan.radius}px`;
    const blockTokens={blockquote:{lineColor:'--md-blockquote-line-color',color:'--md-blockquote-color',background:'--md-blockquote-bg',border:'--md-blockquote-border'},task:{color:'--md-task-color',background:'--md-task-bg',border:'--md-task-border',accent:'--md-task-accent'},rule:{color:'--md-rule-color',background:'--md-rule-bg',border:'--md-rule-border'},image:{captionColor:'--md-image-caption-color',color:'--md-image-color',background:'--md-image-bg',border:'--md-image-border'},list:{color:'--md-list-color',markerColor:'--md-list-marker-color',background:'--md-list-bg',border:'--md-list-border'},formula:{color:'--md-formula-color',background:'--md-formula-bg',border:'--md-formula-border'},diagram:{color:'--md-diagram-color',background:'--md-diagram-bg',border:'--md-diagram-border'}};
    Object.entries(blockTokens).forEach(([block,tokens])=>Object.entries(tokens).forEach(([key,varName])=>{if(md[block]?.[key])css[varName]=md[block][key];}));
    const callouts=md.callout||{};Object.entries(callouts).forEach(([kind,values])=>{for(const [key,suffix] of [['color','color'],['background','bg'],['border','border']])if(values?.[key])css[`--md-callout-${kind}-${suffix}`]=values[key];});
    const icons=app.icons||{};if(icons.strokeWidth)css['--app-icon-stroke']=icons.strokeWidth;for(const key of ['default','muted','accent','active','disabled','danger'])if(icons[key])css[`--app-icon-${key}`]=icons[key];
    const tree=app.fileTree||{};for(const [key,variable] of [['folderClosed','folder'],['folderOpen','folder-open'],['markdownFile','markdown'],['imageFile','image'],['otherFile','other']])if(tree[key]?.color)css[`--app-file-${variable}`]=tree[key].color;
    const alpha=app.window?.backgroundOpacity??1;
    if(colors.window&&alpha<1){const match=colors.window.match(/^#([0-9a-f]{6})$/i);if(match){const n=parseInt(match[1],16);css['--app-window']=`rgba(${n>>16},${(n>>8)&255},${n&255},${alpha})`;}}
    css['--app-window-opacity']=alpha;
    Object.entries(css).forEach(([key,value])=>{root.style.setProperty(key,String(value));appliedTokens.add(key);});
    root.dataset.themeZebra=String(Boolean(md.table?.zebra));
    root.dataset.themeIconStyle=icons.style||'outline';for(const [key,dataKey] of [['folderClosed','folderClosed'],['folderOpen','folderOpen'],['markdownFile','markdown'],['imageFile','image'],['otherFile','other']])root.dataset[`fileIcon${dataKey[0].toUpperCase()}${dataKey.slice(1)}Style`]=tree[key]?.style||'outline';
    css['--app-icon-fill']=icons.style==='filled'?'currentColor':'none';
    root.style.setProperty('--app-icon-fill',css['--app-icon-fill']);appliedTokens.add('--app-icon-fill');
    root.dataset.palette=id;root.dataset.theme=mode;root.dataset.highlightPalette=highlightTheme.id;root.dataset.highlightMode=resolvedHighlightMode;
    loadThemeFonts(theme).catch(error=>report(`主题已应用，字体加载失败：${error.message}`,true));
    updateCurrentModePreviews();syncCards();syncHighlightSettings();
  }
  async function loadThemeFonts(theme){
    if(!theme.fontData||typeof FontFace==='undefined')return;
    const registered=window.alineThemeFonts||(window.alineThemeFonts=new Map());
    for(const font of theme.fonts||[]){const key=`${theme.id}:${font.family}`;if(registered.has(key))continue;const bytes=theme.fontData[font.path];if(!bytes)continue;const face=new FontFace(font.family,bytes.buffer.slice(bytes.byteOffset,bytes.byteOffset+bytes.byteLength));await face.load();document.fonts.add(face);registered.set(key,face);}
  }
  window.getThemeCatalog=()=>catalog.map(({id,name})=>({id,name}));
  window.hasThemeFamily=id=>catalog.some(theme=>theme.id===id);
  window.onThemePaletteChanged=(id,mode)=>applyTheme(id,mode);
  window.refreshThemeGallery=refreshThemeGallery;
  async function refreshThemeGallery(){
    const sequence=++refreshSequence;report('正在读取主题文件夹…');
    try{
      await migrateOldThemes();
      const indexResponse=await fetch(`./themes/index.json?refresh=${Date.now()}`,{cache:'no-store'});if(!indexResponse.ok)throw new Error('无法读取内置主题索引');
      const index=await indexResponse.json();if(index.schemaVersion!==3||!Array.isArray(index.themes))throw new Error('内置主题索引版本无效');
      const loaded=await Promise.all(index.themes.map(entry=>readTheme(`./themes/${entry.id}`,entry.id,true)));
      if(sequence!==refreshSequence)return;
      const installed=await readCustomThemes(),ids=new Set(loaded.map(theme=>theme.id));
      catalog=[...loaded,...installed.filter(theme=>!ids.has(theme.id))].sort((a,b)=>{const rankA=builtinIds.indexOf(a.id),rankB=builtinIds.indexOf(b.id);if(rankA>=0||rankB>=0)return (rankA<0?builtinIds.length:rankA)-(rankB<0?builtinIds.length:rankB);return a.name.localeCompare(b.name,'zh-CN');});
      const id=document.documentElement.dataset.palette||'github',mode=document.documentElement.dataset.theme||'light';
      if(!highlightSettingsLoaded){
        try{
          const savedTheme=localStorage.getItem('aline.highlightTheme');
          const legacyTheme=localStorage.getItem('aline.highlightPalette');
          highlightThemeSetting=savedTheme??(legacyTheme?'':'follow-app');
          if(legacyTheme&&!savedTheme){highlightThemeSetting=legacyTheme;localStorage.setItem('aline.highlightTheme',legacyTheme);localStorage.removeItem('aline.highlightPalette');}
          highlightModeSetting=localStorage.getItem('aline.highlightMode')||'follow-app';
        }catch{highlightThemeSetting='follow-app';highlightModeSetting='follow-app';}
        if(highlightThemeSetting!=='follow-app'&&!catalog.some(item=>item.id===highlightThemeSetting))highlightThemeSetting='follow-app';
        if(highlightModeSetting!=='follow-app'&&!modes.some(([key])=>key===highlightModeSetting))highlightModeSetting='follow-app';
        highlightSettingsLoaded=true;
      }
      applyTheme(id,mode);const github=catalog.find(item=>item.id==='github');for(const item of catalog){item.resolved={markdown:merge(modeConfig(github?.configs.markdown||{},mode),modeConfig(item.configs?.markdown||{},mode)),application:merge(modeConfig(github?.configs.application||{},mode),modeConfig(item.configs?.application||{},mode)),highlight:merge(modeConfig(github?.configs.highlight||{},mode),modeConfig(item.configs?.highlight||{},mode))};}
      renderCards();syncCards();syncHighlightSettings();report(`已加载 ${catalog.length} 个主题`);
    }catch(error){if(sequence!==refreshSequence)return;renderCards();report(`刷新失败：${error.message}`,true);}
  }
  async function decodePreview(bytes,extension){
    if(!validImage(bytes,extension))throw new Error('预览图片数据与扩展名不匹配');
    if(extension==='svg'){const xml=new DOMParser().parseFromString(new TextDecoder().decode(bytes),'image/svg+xml');if(xml.querySelector('parsererror')||xml.documentElement.localName!=='svg'||xml.querySelector('script,foreignObject,[href],[xlink\\:href],[style*=url]')||[...xml.querySelectorAll('*')].some(node=>[...node.attributes].some(attribute=>/^on/i.test(attribute.name))))throw new Error('SVG 预览必须自包含且不含脚本、外部引用或事件');return;}
    if(typeof createImageBitmap==='function'){const mime={png:'image/png',jpg:'image/jpeg',jpeg:'image/jpeg',webp:'image/webp'}[extension];const bitmap=await createImageBitmap(new Blob([bytes],{type:mime}));bitmap.close();}
  }
  function validImage(bytes,extension){if(extension==='svg'){const text=new TextDecoder().decode(bytes).trimStart();return /^<svg\b|^<\?xml[\s\S]*?<svg\b/i.test(text);}if(extension==='png')return bytes[0]===137&&bytes[1]===80&&bytes[2]===78&&bytes[3]===71;if(extension==='jpg'||extension==='jpeg')return bytes[0]===255&&bytes[1]===216;if(extension==='webp')return new TextDecoder().decode(bytes.subarray(0,4))==='RIFF'&&new TextDecoder().decode(bytes.subarray(8,12))==='WEBP';return false;}
  function updateCurrentModePreviews(){const mode=document.documentElement.dataset.theme||'light';for(const button of document.querySelectorAll('.theme-preview-open')){const theme=catalog.find(item=>item.id===button.dataset.themeId);if(!theme)continue;const src=theme.previewData?.[mode]||`./themes/${encodeURIComponent(theme.id)}/${theme.previews[mode].split('/').map(encodeURIComponent).join('/')}`,label=modes.find(([key])=>key===mode)?.[1]||'白天';button.dataset.previewMode=mode;button.setAttribute('aria-label',`预览 ${theme.name} 主题，当前为${label}模式`);const image=button.querySelector('img');if(image){image.src=src;image.alt=`${theme.name} ${label}模式界面预览`;}}}
  function closeDialog(){document.querySelectorAll('.theme-library-backdrop').forEach(dialog=>dialog.remove());activePreview=null;}
  function previewSource(theme,mode){return theme.previewData?.[mode]||`./themes/${encodeURIComponent(theme.id)}/${theme.previews[mode].split('/').map(encodeURIComponent).join('/')}`;}
  function renderPreview(){const dialog=$('.preview-backdrop'),theme=catalog.find(item=>item.id===activePreview?.themeId);if(!dialog||!theme)return;const [,label]=modes.find(([mode])=>mode===activePreview.mode)||modes[0];dialog.querySelector('.theme-preview-dialog').setAttribute('aria-label',`${theme.name} ${label}模式主题预览`);dialog.querySelector('h2').textContent=`${theme.name} · ${label}`;const image=dialog.querySelector('.theme-preview-image');image.src=previewSource(theme,activePreview.mode);image.alt=`${theme.name} ${label}模式主题预览`;dialog.querySelector('.theme-preview-mode').textContent=label;}
  function showPreview(themeId,mode){closeDialog();activePreview={themeId,mode};const theme=catalog.find(item=>item.id===themeId),src=theme?escapeHtml(previewSource(theme,mode)):'';const dialog=document.createElement('div');dialog.className='theme-library-backdrop preview-backdrop';dialog.innerHTML=`<section class="theme-preview-dialog" role="dialog" aria-modal="true"><button class="theme-library-close" type="button" aria-label="关闭预览"><svg viewBox="0 0 16 16" aria-hidden="true"><path d="m4 4 8 8M12 4l-8 8"/></svg></button><h2></h2><div class="theme-preview-stage"><button type="button" class="theme-preview-step previous" aria-label="切换到上一种模式"><svg viewBox="0 0 16 16" aria-hidden="true"><path d="m10 3-5 5 5 5"/></svg></button><img class="theme-preview-image" src="${src}" alt=""><button type="button" class="theme-preview-step next" aria-label="切换到下一种模式"><svg viewBox="0 0 16 16" aria-hidden="true"><path d="m6 3 5 5-5 5"/></svg></button></div><div class="theme-preview-mode"></div></section>`;document.body.append(dialog);renderPreview();}
  function movePreview(step){if(!activePreview)return;const index=modes.findIndex(([mode])=>mode===activePreview.mode);activePreview.mode=modes[(index+step+modes.length)%modes.length][0];renderPreview();}
  function dataUrl(bytes,mime){let binary='';for(let i=0;i<bytes.length;i+=0x8000)binary+=String.fromCharCode(...bytes.subarray(i,Math.min(i+0x8000,bytes.length)));return `data:${mime};base64,${btoa(binary)}`;}
  async function extractEntry(bytes,entry){const view=new DataView(bytes.buffer,bytes.byteOffset,bytes.byteLength),offset=entry.localOffset;if(offset+30>bytes.length||view.getUint32(offset,true)!==0x04034b50)throw new Error(`ZIP 数据损坏：${entry.name}`);const nameLength=view.getUint16(offset+26,true),extraLength=view.getUint16(offset+28,true),start=offset+30+nameLength+extraLength,end=start+entry.compressedSize;if(end>bytes.length)throw new Error(`ZIP 数据截断：${entry.name}`);const compressed=bytes.subarray(start,end);let result;if(entry.method===0)result=compressed;else{if(entry.method!==8||typeof DecompressionStream==='undefined')throw new Error('主题 ZIP 使用了当前原型不支持的压缩方式');const stream=new Blob([compressed]).stream().pipeThrough(new DecompressionStream('deflate-raw'));result=new Uint8Array(await new Response(stream).arrayBuffer());}if(result.length!==entry.size)throw new Error(`ZIP 解压大小不匹配：${entry.name}`);return result;}
  function zipEntries(bytes){const view=new DataView(bytes.buffer,bytes.byteOffset,bytes.byteLength);let end=-1;for(let offset=bytes.length-22;offset>=Math.max(0,bytes.length-65557);offset--)if(view.getUint32(offset,true)===0x06054b50){end=offset;break;}if(end<0)throw new Error('文件不是有效 ZIP，或 ZIP 目录已损坏');const count=view.getUint16(end+10,true),directoryOffset=view.getUint32(end+16,true);if(count===0xffff||directoryOffset===0xffffffff)throw new Error('暂不支持 ZIP64 主题包');const entries=new Map();let offset=directoryOffset,total=0;for(let n=0;n<count;n++){if(offset+46>bytes.length||view.getUint32(offset,true)!==0x02014b50)throw new Error('ZIP 中央目录格式错误');const method=view.getUint16(offset+10,true),compressedSize=view.getUint32(offset+20,true),size=view.getUint32(offset+24,true),nameLength=view.getUint16(offset+28,true),extraLength=view.getUint16(offset+30,true),commentLength=view.getUint16(offset+32,true),localOffset=view.getUint32(offset+42,true),name=new TextDecoder().decode(bytes.subarray(offset+46,offset+46+nameLength));if(!safePath(name.replace(/\/$/,''))||name.startsWith('/')||name.includes('\\'))throw new Error('主题包包含越界路径');if(!name.endsWith('/')){if(entries.has(name))throw new Error(`ZIP 文件重复：${name}`);entries.set(name,{name,method,compressedSize,size,localOffset});total+=size;}offset+=46+nameLength+extraLength+commentLength;}if(total>120*1024*1024)throw new Error('主题包解压后超过 120 MB');return entries;}
  function readFontFamily(bytes){const view=new DataView(bytes.buffer,bytes.byteOffset,bytes.byteLength),tag=String.fromCharCode(...bytes.subarray(0,4));if(tag==='ttcf'||tag==='wOF2'||tag==='wOFF'||(tag!=='OTTO'&&view.getUint32(0)!==0x00010000))throw new Error('只支持静态 TTF/OTF 字体，不支持 TTC、可变或 Web 字体');const count=view.getUint16(4);let nameOffset=-1,nameLength=0;for(let i=0;i<count;i++){const p=12+i*16,t=String.fromCharCode(...bytes.subarray(p,p+4)),o=view.getUint32(p+8),l=view.getUint32(p+12);if(t==='fvar')throw new Error('不支持可变字体');if(t==='name'){nameOffset=o;nameLength=l;}}if(nameOffset<0||nameOffset+nameLength>bytes.length)throw new Error('字体缺少有效名称表');const countNames=view.getUint16(nameOffset+2),strings=nameOffset+view.getUint16(nameOffset+4);const names=[];for(let i=0;i<countNames;i++){const p=nameOffset+6+i*12,platform=view.getUint16(p),nameId=view.getUint16(p+6),length=view.getUint16(p+8),off=strings+view.getUint16(p+10);if((nameId===16||nameId===1)&&off+length<=bytes.length){const part=bytes.subarray(off,off+length);const value=platform===0||platform===3?Array.from({length:Math.floor(part.length/2)},(_,n)=>String.fromCharCode(part[n*2]*256+part[n*2+1])).join(''):new TextDecoder('windows-1252').decode(part);if(value.trim())names.push({id:nameId,value:value.trim()});}}return names.find(item=>item.id===16)?.value||names.find(item=>item.id===1)?.value||null;}
  async function validateThemeZip(file){
    if(!/\.zip$/i.test(file.name))throw new Error('请选择 .zip 格式的主题包');if(file.size>100*1024*1024)throw new Error('主题 ZIP 大小不能超过 100 MB');
    const bytes=new Uint8Array(await file.arrayBuffer()),entries=zipEntries(bytes),manifestEntry=[...entries.values()].find(entry=>/(^|\/)theme\.json$/i.test(entry.name));if(!manifestEntry)throw new Error('主题包缺少 theme.json');
    const root=manifestEntry.name==='theme.json'?'':manifestEntry.name.slice(0,-'theme.json'.length);if(root&&root.split('/').filter(Boolean).length!==1)throw new Error('theme.json 只能放在 ZIP 根目录或单层主题目录中');
    const manifest=JSON.parse(new TextDecoder().decode(await extractEntry(bytes,manifestEntry)));if(root&&root.slice(0,-1)!==manifest.id)throw new Error('ZIP 顶层主题目录名必须与主题 id 一致');validateManifest(manifest,manifest.id);if(catalog.some(theme=>theme.id===manifest.id))throw new Error(`主题 id 已存在：${manifest.id}`);
    const getFile=async path=>{if(!safePath(path))throw new Error(`主题资源路径无效：${path}`);const entry=entries.get(`${root}${path}`);if(!entry)throw new Error(`主题包缺少文件：${path}`);return extractEntry(bytes,entry);};
    const markdown=JSON.parse(new TextDecoder().decode(await getFile(manifest.files.markdown))),application=JSON.parse(new TextDecoder().decode(await getFile(manifest.files.application))),highlight=JSON.parse(new TextDecoder().decode(await getFile(manifest.files.highlight)));validateConfig(markdown,'markdown');validateConfig(application,'application');validateConfig(highlight,'highlight');
    const previewData={},previewPaths=new Set();for(const [mode] of modes){const relative=manifest.previews[mode];if(previewPaths.has(relative))throw new Error('白天、夜间、护眼必须分别提供三张预览图');previewPaths.add(relative);const entry=entries.get(`${root}${relative}`);if(!entry||entry.size>1024*1024)throw new Error(`缺少有效的 ${mode} 预览图：${relative}`);const image=await extractEntry(bytes,entry),extension=relative.split('.').pop().toLowerCase();try{await decodePreview(image,extension);}catch{throw new Error(`预览图格式无效：${relative}`);}const mime={svg:'image/svg+xml',png:'image/png',jpg:'image/jpeg',jpeg:'image/jpeg',webp:'image/webp'}[extension];previewData[mode]=dataUrl(image,mime);}
    let fontTotal=0;const fontData={};for(const font of manifest.fonts||[]){const extension=font.path.split('.').pop().toLowerCase();if(!['ttf','otf'].includes(extension))throw new Error('主题字体仅支持 .ttf 与 .otf');const fontBytes=await getFile(font.path);if(fontBytes.length>20*1024*1024)throw new Error(`字体文件超过 20 MB：${font.path}`);fontTotal+=fontBytes.length;if(fontTotal>80*1024*1024)throw new Error('主题字体总量超过 80 MB');if(readFontFamily(fontBytes)!==font.family)throw new Error(`字体家族名与文件元数据不符：${font.family}`);const license=await getFile(font.licenseFile);if(!new TextDecoder().decode(license).trim())throw new Error(`字体授权文件为空：${font.licenseFile}`);fontData[font.path]=fontBytes;}
    return {...manifest,builtin:false,configs:{markdown,application,highlight},previewData,fontData};
  }
  document.addEventListener('click',event=>{
    const action=event.target.closest?.('[data-theme-action]');if(action){if(action.dataset.themeAction==='open-folder')window.open('./themes/','_blank','noopener');else if(action.dataset.themeAction==='import')$('#theme-zip-input')?.click();else if(action.dataset.themeAction==='refresh')refreshThemeGallery();return;}
    const selector=event.target.closest?.('.theme-family-select');if(selector){window.setPaletteFamily?.(selector.dataset.palette);return;}
    const preview=event.target.closest?.('.theme-preview-open');if(preview){showPreview(preview.dataset.themeId,document.documentElement.dataset.theme||'light');return;}
    if(event.target.closest?.('.theme-preview-step.previous')){movePreview(-1);return;}if(event.target.closest?.('.theme-preview-step.next')){movePreview(1);return;}
    if(event.target.closest?.('.theme-library-close,.theme-library-primary')||event.target.classList?.contains('theme-library-backdrop'))closeDialog();
  });
  document.addEventListener('change',event=>{if(event.target.id==='highlight-palette-select')window.setHighlightPalette?.(event.target.value);});
  document.addEventListener('change',async event=>{if(event.target.id!=='theme-zip-input')return;const file=event.target.files?.[0];event.target.value='';if(!file)return;report(`正在检查 ${file.name}…`);try{const theme=await validateThemeZip(file);await saveCustomTheme(theme);await refreshThemeGallery();report(`“${theme.name}”已通过检查并加入主题列表（原型存储）`);}catch(error){report(`导入失败：${error.message}`,true);}});
  document.addEventListener('click',event=>{if(event.target.closest?.('#settings-nav button[data-page="外观"]'))setTimeout(()=>{if($('#theme-gallery'))refreshThemeGallery();},0);});
  document.addEventListener('keydown',event=>{if(!activePreview){if(event.key==='Escape')closeDialog();return;}if(event.key==='ArrowLeft'){movePreview(-1);event.preventDefault();}else if(event.key==='ArrowRight'){movePreview(1);event.preventDefault();}else if(event.key==='Escape')closeDialog();});
  refreshThemeGallery();
})();
