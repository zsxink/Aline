// 一期语法的展示夹具；source 与 html 显式配对，供原型切换视图。
(() => {
  const overviewSource = `# Aline · Markdown 写作示例

从一段文字开始，把想法整理成清晰的文档。

[toc]

## 行内格式

这是一段普通正文。你可以使用 **粗体强调**、*斜体说明*、~~删除线~~，或用 \`inline code\` 标注代码与文件名。

访问 [GitHub](https://github.com)，自动链接 <https://github.com> 也可以直接识别。让写作保持轻快 :sparkles: :heart: :+1:。

## 标题层级

### 三级标题

用于文档中的主要小节。

#### 四级标题

用于展开更具体的内容。

##### 五级标题

用于细节说明。

###### 六级标题

保留完整的六级标题层次。

## 段落与换行

一个自然段承载一个主要意思。中文与 English、数字 2026 都可以自然地出现在同一行。

两个空格结束这一行。  
这里是同一段中的软换行。

---

继续在左侧打开列表、代码、公式与图片示例，查看一期语法的完整呈现。
`;
  const overviewHtml = `<h1 id="welcome">Aline · Markdown 写作示例</h1><p>从一段文字开始，把想法整理成清晰的文档。</p><nav class="toc" aria-label="文档目录"><a href="#inline">行内格式</a><a href="#headings">标题层级</a><a href="#paragraphs">段落与换行</a></nav><h2 id="inline">行内格式</h2><p>这是一段普通正文。你可以使用 <strong>粗体强调</strong>、<em>斜体说明</em>、<del>删除线</del>，或用 <code>inline code</code> 标注代码与文件名。</p><p>访问 <a href="https://github.com">GitHub</a>，自动链接 <a href="https://github.com">https://github.com</a> 也可以直接识别。让写作保持轻快 ✨ ❤️ 👍。</p><h2 id="headings">标题层级</h2><h3>三级标题</h3><p>用于文档中的主要小节。</p><h4>四级标题</h4><p>用于展开更具体的内容。</p><h5>五级标题</h5><p>用于细节说明。</p><h6>六级标题</h6><p>保留完整的六级标题层次。</p><h2 id="paragraphs">段落与换行</h2><p>一个自然段承载一个主要意思。中文与 English、数字 2026 都可以自然地出现在同一行。</p><p>两个空格结束这一行。<br>这里是同一段中的软换行。</p><hr><p>继续在左侧打开列表、代码、公式与图片示例，查看一期语法的完整呈现。</p>`;
  const listsSource = `# 列表与引用

## 无序列表

- 一份清楚的文档
  - 合适的标题层级
  - 简洁的段落表达
- 一个专注的写作环境
  - 素白的编辑背景
  - 克制的界面颜色

## 有序列表

1. 打开一个 Markdown 文件。
2. 在可视化模式中编辑内容。
   1. 点击文字开始编辑。
   2. 用快捷键调整格式。
3. 保存并继续写作。

## 任务列表

- [x] 整理文档结构
- [x] 补充代码与表格
- [ ] 检查公式与图表
  - [x] 行内公式
  - [ ] Mermaid 流程图

## 嵌套引用

> 好的工具，让注意力回到内容本身。
>
> > 保持简单，也保持准确。

## GitHub Alerts

> [!NOTE]
> 补充说明，帮助读者理解当前内容。

> [!TIP]
> 使用 ⌘ K 快速插入或编辑链接。

> [!IMPORTANT]
> 源文本是文档内容的唯一事实来源。

> [!WARNING]
> 关闭未保存的文档前，请先保存修改。

> [!CAUTION]
> 删除操作前，请确认文件与内容。
`;
  const alertIcons = {
    note:'<circle cx="8" cy="8" r="6.25"/><path d="M8 7v4M8 4.5v.2"/>',
    tip:'<path d="M5 10c-4-5 5-10 6-4 1 2-1 3-1 4v2H6v-2ZM6 14h4"/>',
    important:'<path d="M8 1.5 14.5 8 8 14.5 1.5 8Z"/><path d="M8 4.5v4M8 11v.2"/>',
    warning:'<path d="m8 1.5 7 13H1Z"/><path d="M8 6v4M8 12v.2"/>',
    caution:'<path d="M5.5 1.5h5l4 4v5l-4 4h-5l-4-4v-5Z"/><path d="m5.5 5.5 5 5m0-5-5 5"/>'
  };
  const alert = (type,title,text) => `<blockquote class="markdown-alert markdown-alert-${type}"><p class="markdown-alert-title"><svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${alertIcons[type]}</svg> ${title}</p><p>${text}</p></blockquote>`;
  const listsHtml = `<h1>列表与引用</h1><h2>无序列表</h2><ul><li>一份清楚的文档<ul><li>合适的标题层级</li><li>简洁的段落表达</li></ul></li><li>一个专注的写作环境<ul><li>素白的编辑背景</li><li>克制的界面颜色</li></ul></li></ul><h2>有序列表</h2><ol><li>打开一个 Markdown 文件。</li><li>在可视化模式中编辑内容。<ol><li>点击文字开始编辑。</li><li>用快捷键调整格式。</li></ol></li><li>保存并继续写作。</li></ol><h2>任务列表</h2><ul class="task-list"><li><input type="checkbox" checked aria-label="整理文档结构">整理文档结构</li><li><input type="checkbox" checked aria-label="补充代码与表格">补充代码与表格</li><li><input type="checkbox" aria-label="检查公式与图表">检查公式与图表<ul class="task-list"><li><input type="checkbox" checked aria-label="行内公式">行内公式</li><li><input type="checkbox" aria-label="Mermaid 流程图">Mermaid 流程图</li></ul></li></ul><h2>嵌套引用</h2><blockquote><p>好的工具，让注意力回到内容本身。</p><blockquote><p>保持简单，也保持准确。</p></blockquote></blockquote><h2>GitHub Alerts</h2>${alert('note','Note','补充说明，帮助读者理解当前内容。')}${alert('tip','Tip','使用 <kbd>⌘ K</kbd> 快速插入或编辑链接。')}${alert('important','Important','源文本是文档内容的唯一事实来源。')}${alert('warning','Warning','关闭未保存的文档前，请先保存修改。')}${alert('caution','Caution','删除操作前，请确认文件与内容。')}`;
  const codeSource = `# 代码与表格

## 代码块示例

\`\`\`rust
// 保留原文，只修改当前区间。
fn main() {
    let name = "Aline";
    println!("Hello, {}!", name);
}
\`\`\`

\`\`\`typescript
interface Document {
  title: string;
  modified: boolean;
}

const document: Document = {
  title: '写作示例.md',
  modified: false,
};
\`\`\`

## 命令示例

    $ cargo run -p aline
    Opening 写作示例.md

## 表格与对齐

| 元素 | Markdown 语法 | 对齐 | 状态 |
| :--- | :---: | ---: | :---: |
| 标题 | \`# 标题\` | 左侧 | 支持 |
| 粗体 | \`**文字**\` | 居中 | 支持 |
| 行内代码 | \`\`代码\`\` | 右侧 | 支持 |
| 任务列表 | \`- [x] 已完成\` | 左侧 | 支持 |

## 行内代码

使用 \`cargo fmt\` 整理 Rust 源码，文件名可以写成 \`README.md\`。
`;
  const codeHtml = `<h1>代码与表格</h1><h2>代码块示例</h2><div class="code-block"><header>Rust</header><pre><code><span class="tok-comment">// 保留原文，只修改当前区间。</span>\n<span class="tok-keyword">fn</span> <span class="tok-function">main</span>() {\n    <span class="tok-keyword">let</span> name = <span class="tok-string">"Aline"</span>;\n    <span class="tok-function">println!</span>(<span class="tok-string">"Hello, {}!"</span>, name);\n}</code></pre></div><div class="code-block"><header>TypeScript</header><pre><code><span class="tok-keyword">interface</span> <span class="tok-function">Document</span> {\n  title: <span class="tok-keyword">string</span>;\n  modified: <span class="tok-keyword">boolean</span>;\n}\n\n<span class="tok-keyword">const</span> document: Document = {\n  title: <span class="tok-string">'写作示例.md'</span>,\n  modified: <span class="tok-keyword">false</span>,\n};</code></pre></div><h2>命令示例</h2><pre><code>$ cargo run -p aline\nOpening 写作示例.md</code></pre><h2>表格与对齐</h2><table><thead><tr><th>元素</th><th style="text-align:center">Markdown 语法</th><th style="text-align:right">对齐</th><th style="text-align:center">状态</th></tr></thead><tbody><tr><td>标题</td><td style="text-align:center"><code># 标题</code></td><td style="text-align:right">左侧</td><td style="text-align:center">支持</td></tr><tr><td>粗体</td><td style="text-align:center"><code>**文字**</code></td><td style="text-align:right">居中</td><td style="text-align:center">支持</td></tr><tr><td>行内代码</td><td style="text-align:center"><code>\`代码\`</code></td><td style="text-align:right">右侧</td><td style="text-align:center">支持</td></tr><tr><td>任务列表</td><td style="text-align:center"><code>- [x] 已完成</code></td><td style="text-align:right">左侧</td><td style="text-align:center">支持</td></tr></tbody></table><h2>行内代码</h2><p>使用 <code>cargo fmt</code> 整理 Rust 源码，文件名可以写成 <code>README.md</code>。</p>`;
  const mathSource = `# 数学公式与 Mermaid

## 行内公式

勾股定理 $a^2 + b^2 = c^2$ 描述了直角三角形三条边之间的关系。

## 块级公式

$$
f(x) = \\frac{1}{\\sigma\\sqrt{2\\pi}} e^{-\\frac{(x-\\mu)^2}{2\\sigma^2}}
$$

$$
\\sum_{k=1}^{n} k = \\frac{n(n+1)}{2}
$$

## Mermaid 流程图

\`\`\`mermaid
flowchart LR
    A[打开文档] --> B{编辑模式}
    B -->|可视化| C[就地编辑]
    B -->|源码| D[编辑 Markdown]
    C --> E[保存文档]
    D --> E
\`\`\`

## Mermaid 时序图

\`\`\`mermaid
sequenceDiagram
    participant U as 用户
    participant E as 编辑器
    participant F as 文件系统
    U->>E: 修改内容
    E->>E: 更新文档
    E->>F: 保存修改
    F-->>E: 保存成功
\`\`\`
`;
  const flow = `<svg class="mermaid-svg" viewBox="0 0 760 240" role="img" aria-label="打开文档后选择可视化或源码模式，最终保存文档"><defs><marker id="flow-arrow" markerWidth="8" markerHeight="8" refX="7" refY="4" orient="auto"><path d="M0 0L8 4L0 8" fill="#57606a"/></marker></defs><g fill="none" stroke="#57606a" stroke-width="1.5" marker-end="url(#flow-arrow)"><path d="M149 120H219"/><path d="M309 120L370 55H425"/><path d="M309 120L370 185H425"/><path d="M551 55H592L635 110"/><path d="M551 185H592L635 130"/></g><g fill="#f6f8fa" stroke="#d0d7de" stroke-width="1.5"><rect x="25" y="95" width="124" height="50" rx="6"/><path d="M264 75L309 120L264 165L219 120Z"/><rect x="425" y="30" width="126" height="50" rx="6"/><rect x="425" y="160" width="126" height="50" rx="6"/><rect x="635" y="95" width="104" height="50" rx="6"/></g><g fill="#24292f" font-family="system-ui,sans-serif" font-size="15" text-anchor="middle"><text x="87" y="126">打开文档</text><text x="264" y="126">编辑模式</text><text x="488" y="61">就地编辑</text><text x="488" y="191">编辑 Markdown</text><text x="687" y="126">保存文档</text><text x="367" y="45" font-size="12" fill="#57606a">可视化</text><text x="367" y="210" font-size="12" fill="#57606a">源码</text></g></svg>`;
  const sequence = `<svg class="mermaid-svg" viewBox="0 0 760 290" role="img" aria-label="用户修改内容，编辑器更新文档并保存至文件系统"><defs><marker id="seq-arrow" markerWidth="8" markerHeight="8" refX="7" refY="4" orient="auto"><path d="M0 0L8 4L0 8" fill="#57606a"/></marker></defs><g stroke="#d0d7de" stroke-dasharray="5 5"><path d="M110 60V275M380 60V275M650 60V275"/></g><g fill="#f6f8fa" stroke="#d0d7de"><rect x="55" y="15" width="110" height="44" rx="5"/><rect x="325" y="15" width="110" height="44" rx="5"/><rect x="595" y="15" width="110" height="44" rx="5"/></g><g fill="#24292f" font-family="system-ui,sans-serif" font-size="15" text-anchor="middle"><text x="110" y="43">用户</text><text x="380" y="43">编辑器</text><text x="650" y="43">文件系统</text><text x="245" y="99" font-size="13">修改内容</text><text x="449" y="145" font-size="13">更新文档</text><text x="515" y="195" font-size="13">保存修改</text><text x="515" y="245" font-size="13">保存成功</text></g><g fill="none" stroke="#57606a" stroke-width="1.5" marker-end="url(#seq-arrow)"><path d="M110 110H380M380 135h90v30h-90M380 205H650"/><path d="M650 255H380" stroke-dasharray="5 4"/></g></svg>`;
  const mathHtml = `<h1>数学公式与 Mermaid</h1><h2>行内公式</h2><p>勾股定理 <math aria-label="a 平方加 b 平方等于 c 平方"><msup><mi>a</mi><mn>2</mn></msup><mo>+</mo><msup><mi>b</mi><mn>2</mn></msup><mo>=</mo><msup><mi>c</mi><mn>2</mn></msup></math> 描述了直角三角形三条边之间的关系。</p><h2>块级公式</h2><div class="math-block"><math display="block" aria-label="正态分布概率密度函数"><mrow><mi>f</mi><mo>(</mo><mi>x</mi><mo>)</mo></mrow><mo>=</mo><mfrac><mn>1</mn><mrow><mi>σ</mi><msqrt><mrow><mn>2</mn><mi>π</mi></mrow></msqrt></mrow></mfrac><msup><mi>e</mi><mrow><mo>−</mo><mfrac><msup><mrow><mo>(</mo><mi>x</mi><mo>−</mo><mi>μ</mi><mo>)</mo></mrow><mn>2</mn></msup><mrow><mn>2</mn><msup><mi>σ</mi><mn>2</mn></msup></mrow></mfrac></mrow></msup></math></div><div class="math-block"><math display="block" aria-label="从一到 n 的自然数之和等于 n 乘 n 加一除以二"><munderover><mo>∑</mo><mrow><mi>k</mi><mo>=</mo><mn>1</mn></mrow><mi>n</mi></munderover><mi>k</mi><mo>=</mo><mfrac><mrow><mi>n</mi><mo>(</mo><mi>n</mi><mo>+</mo><mn>1</mn><mo>)</mo></mrow><mn>2</mn></mfrac></math></div><h2>Mermaid 流程图</h2><div class="diagram-preview">${flow}</div><h2>Mermaid 时序图</h2><div class="diagram-preview">${sequence}</div>`;
  const htmlSource = `# 图片、HTML 与脚注

[toc]

## 本地图片

![层叠的山丘与一轮日光](./assets/showcase-landscape.svg)

图片引用支持本地相对路径、file: 与 data: URI。一期不加载远程图片。

## HTML 行内样式

<strong>粗体</strong>、<em>斜体</em>、<del>删除线</del>、<u>下划线</u>、<mark>高亮标记</mark>、<code>代码</code>、<kbd>⌘ K</kbd> 和 <a href="https://github.com">链接</a>。

实体解码：A &amp; B、&#65;、&#x41;。

## HTML 块文本

<div>
  <p>这是 HTML 块中的<strong>可见文字</strong>。</p>
  <p>块级标签产生换行，行内样式正常保留。:sparkles:</p>
  <!-- 此注释不进入展示文本 -->
</div>

## 过滤标签

<script>window.example = true;</script>

过滤标签按字面文本展示，不执行其中的内容。

## 脚注

一份清晰的说明可以把补充信息放在脚注里[^note]，也可以添加第二条引用[^source]。

[^note]: 脚注以编号显示，并支持返回正文。
[^source]: Aline 一期功能清单是本示例的范围依据。
`;
  const htmlHtml = `<h1 id="media-title">图片、HTML 与脚注</h1><nav class="toc" aria-label="文档目录"><a href="#local-image">本地图片</a><a href="#html-inline">HTML 行内样式</a><a href="#html-block">HTML 块文本</a><a href="#filtered-tags">过滤标签</a><a href="#footnotes-heading">脚注</a></nav><h2 id="local-image">本地图片</h2><p><img class="showcase-image" src="./assets/showcase-landscape.svg" alt="层叠的山丘与一轮日光" width="800" height="360"></p><p>图片引用支持本地相对路径、<code>file:</code> 与 <code>data:</code> URI。一期不加载远程图片。</p><h2 id="html-inline">HTML 行内样式</h2><p><strong>粗体</strong>、<em>斜体</em>、<del>删除线</del>、<u>下划线</u>、<mark>高亮标记</mark>、<code>代码</code>、<kbd>⌘ K</kbd> 和 <a href="https://github.com">链接</a>。</p><p>实体解码：A &amp; B、A、A。</p><h2 id="html-block">HTML 块文本</h2><p>这是 HTML 块中的<strong>可见文字</strong>。</p><p>块级标签产生换行，行内样式正常保留。✨</p><h2 id="filtered-tags">过滤标签</h2><p><code>&lt;script&gt;window.example = true;&lt;/script&gt;</code></p><p>过滤标签按字面文本展示，不执行其中的内容。</p><h2 id="footnotes-heading">脚注</h2><p>一份清晰的说明可以把补充信息放在脚注里<sup id="fnref-note"><a href="#fn-note">1</a></sup>，也可以添加第二条引用<sup id="fnref-source"><a href="#fn-source">2</a></sup>。</p><section class="footnotes"><hr><ol><li id="fn-note">脚注以编号显示，并支持返回正文。 <a href="#fnref-note" aria-label="返回第一条脚注引用">↩</a></li><li id="fn-source">Aline 一期功能清单是本示例的范围依据。 <a href="#fnref-source" aria-label="返回第二条脚注引用">↩</a></li></ol></section>`;
  const readmeSource = `# Aline Markdown 示例

这里集中展示 Aline 一期支持的 Markdown 写作与预览能力。点击左侧文件树中的示例文档即可查看对应效果。

## 示例目录

- \`01-基础语法.md\`：标题、段落、行内格式与链接。
- \`02-列表与引用.md\`：嵌套列表、任务列表、引用与提示块。
- \`03-代码与表格.md\`：代码块、语言选择、复制与表格。
- \`04-公式与图表.md\`：数学公式及图表示例。
- \`05-图片与HTML.md\`：本地图片、HTML 与脚注。

## 使用方式

在文件树中展开文件夹并选择 Markdown 文件；代码块右上角可以切换语言或复制代码。示例素材位于 \`images/\` 文件夹中。
`;
  const readmeHtml = `<h1>Aline Markdown 示例</h1><p>这里集中展示 Aline 一期支持的 Markdown 写作与预览能力。点击左侧文件树中的示例文档即可查看对应效果。</p><h2>示例目录</h2><ul><li><code>01-基础语法.md</code>：标题、段落、行内格式与链接。</li><li><code>02-列表与引用.md</code>：嵌套列表、任务列表、引用与提示块。</li><li><code>03-代码与表格.md</code>：代码块、语言选择、复制与表格。</li><li><code>04-公式与图表.md</code>：数学公式及图表示例。</li><li><code>05-图片与HTML.md</code>：本地图片、HTML 与脚注。</li></ul><h2>使用方式</h2><p>在文件树中展开文件夹并选择 Markdown 文件；代码块右上角可以切换语言或复制代码。示例素材位于 <code>images/</code> 文件夹中。</p>`;
  window.ALINE_DEMOS = [
    {id:'overview',name:'01-基础语法.md',source:overviewSource,html:overviewHtml},
    {id:'lists',name:'02-列表与引用.md',source:listsSource,html:listsHtml},
    {id:'code',name:'03-代码与表格.md',source:codeSource,html:codeHtml},
    {id:'math',name:'04-公式与图表.md',source:mathSource,html:mathHtml},
    {id:'html',name:'05-图片与HTML.md',source:htmlSource,html:htmlHtml},
    {id:'readme',name:'README.md',source:readmeSource,html:readmeHtml}
  ];
})();
