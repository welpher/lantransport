export type Locale = "zh" | "en";

export interface Copy {
  tagline: string;
  running: string;
  stopped: string;
  folder: string;
  choose: string;
  reveal: string;
  noFolder: string;
  port: string;
  lan: string;
  password: string;
  passwordHint: string;
  remember: string;
  start: string;
  stop: string;
  starting: string;
  urls: string;
  copy: string;
  copied: string;
  qr: string;
  open: string;
  autoStart: string;
  tray: string;
  uploads: string;
  uploadKey: string;
  regenerate: string;
  copyLink: string;
  maxUpload: string;
  uploadHint: string;
  security: string;
  version: string;
  errors: Record<string, string>;
}

export const copy: Record<Locale, Copy> = {
  zh: {
    tagline: "把任意文件夹变成一个局域网页面",
    running: "共享中",
    stopped: "未共享",
    folder: "文件夹",
    choose: "选择…",
    reveal: "打开",
    noFolder: "还没有选择文件夹",
    port: "端口",
    lan: "允许局域网访问",
    password: "访问口令（可选）",
    passwordHint: "用户名固定为 share",
    remember: "记住口令",
    start: "开始共享",
    stop: "停止共享",
    starting: "启动中…",
    urls: "访问地址",
    copy: "复制",
    copied: "已复制",
    qr: "二维码",
    open: "在浏览器中打开",
    autoStart: "打开 LanTransport 时自动开始共享",
    tray: "关闭窗口后继续在托盘运行",
    uploads: "允许凭 key 上传",
    uploadKey: "上传 key",
    regenerate: "换一个",
    copyLink: "复制上传链接",
    maxUpload: "单个文件上限（MB）",
    uploadHint: "把链接和 key 一起发给对方。对方打开链接后可以把文件拖进当前目录。",
    security: "拿到链接的人可以浏览和下载；只有拿到上传 key 的人能添加文件，任何人都不能删除或修改。",
    version: "版本",
    errors: {
      port_in_use: "端口被占用了，换一个端口试试。",
      invalid_root: "文件夹不存在或无法访问。",
      permission_denied: "没有权限，试试 1024 以上的端口。",
      invalid_port: "端口必须在 1–65535 之间。",
      short_password: "口令至少 4 个字符。",
      empty_password: "请输入口令，或取消「记住口令」。",
      no_folder: "先选择要共享的文件夹。",
      not_running: "先开始共享，才能复制上传链接。",
      io: "启动失败，请重试。",
    },
  },
  en: {
    tagline: "Turn any folder into a LAN file page",
    running: "Sharing",
    stopped: "Not sharing",
    folder: "Folder",
    choose: "Choose…",
    reveal: "Open",
    noFolder: "No folder selected",
    port: "Port",
    lan: "Allow LAN access",
    password: "Password (optional)",
    passwordHint: "Username is always share",
    remember: "Remember password",
    start: "Start sharing",
    stop: "Stop sharing",
    starting: "Starting…",
    urls: "Links",
    copy: "Copy",
    copied: "Copied",
    qr: "QR code",
    open: "Open in browser",
    autoStart: "Start sharing when LanTransport opens",
    tray: "Keep running in the tray after closing the window",
    uploads: "Allow uploads with a key",
    uploadKey: "Upload key",
    regenerate: "Regenerate",
    copyLink: "Copy upload link",
    maxUpload: "Max file size (MB)",
    uploadHint: "Send the link and key together. Recipients can drop files into the folder they are viewing.",
    security:
      "Anyone on this network with the link can browse and download. Only people with the upload key can add files; nothing can be deleted.",
    version: "Version",
    errors: {
      port_in_use: "That port is taken. Try another one.",
      invalid_root: "That folder does not exist or cannot be read.",
      permission_denied: "Permission denied. Try a port above 1024.",
      invalid_port: "Port must be between 1 and 65535.",
      short_password: "Use at least 4 characters.",
      empty_password: "Enter a password, or uncheck Remember password.",
      no_folder: "Choose a folder first.",
      not_running: "Start sharing before copying an upload link.",
      io: "Could not start. Please try again.",
    },
  },
};
