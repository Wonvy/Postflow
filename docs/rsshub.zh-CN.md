# 部署自己的 RSSHub

[English](rsshub.md)

[RSSHub](https://github.com/DIYgod/RSSHub) 能为几千个没有 RSS 的网站生成订阅：视频频道、社交平台、论坛、应用商店、项目主页等等。
ShibaPost 可以搜索它的路由、帮你把参数填好（**添加订阅 → RSSHub**），但订阅内容要由一台运行着 RSSHub 的服务器提供，
这台服务器叫做一个**实例**。

官方的公共实例 `rsshub.app` 只用来体验，它给每个请求都加了防机器人验证，ShibaPost 这样的阅读器用不了。
自己部署一个只要几分钟，下载大约 130 MB。

> 你可能根本用不着 RSSHub：大多数博客和新闻网站都有订阅源，ShibaPost 自己就能找到；YouTube 的频道和播放列表、Telegram 公开频道，
> 以及**添加订阅 → 网址**里列出的常用网站（GitHub、GitLab、Reddit、Mastodon、Steam、PyPI 等）也能直接订阅，在那里粘贴地址即可。

- [选择部署在哪里](#选择部署在哪里)
- [部署在自己的电脑上](#部署在自己的电脑上)
- [部署在 NAS 上](#部署在-nas-上)
- [部署在云服务器上](#部署在云服务器上)
- [在 ShibaPost 里填写](#在-shibapost-里填写)
- [需要额外配置的路由](#需要额外配置的路由)
- [更新](#更新)
- [排查问题](#排查问题)

## 选择部署在哪里

| 部署在 | 适合 | ShibaPost 里填的地址 |
| --- | --- | --- |
| 自己的电脑 | 先试试；只在一台电脑上用 | `http://localhost:1200` |
| NAS 或家里的服务器 | 家里所有设备都能用，一直开着 | `http://<NAS 的地址>:1200` |
| 云服务器 | 所有设备在哪儿都能用 | `https://rsshub.你的域名` |

三种方式都用官方的 Docker 镜像 `diygod/rsshub`。如果 ShibaPost 里某条路由标着**需浏览器渲染**，改用
`diygod/rsshub:chromium-bundled` 镜像（约 400–500 MB），其他步骤不变。

## 部署在自己的电脑上

1. 安装并打开 [Docker Desktop](https://www.docker.com/products/docker-desktop/)。
2. 运行：

   ```bash
   docker run -d --name rsshub --restart unless-stopped -p 1200:1200 diygod/rsshub
   ```

3. 在浏览器里打开 <http://localhost:1200>，能看到 RSSHub 的欢迎页就成功了。

ShibaPost 里填：`http://localhost:1200`。电脑开着时订阅才会更新。想让同一个网络里的其他设备也用上，把 `localhost`
换成这台电脑在局域网里的 IP，比如 `http://192.168.1.10:1200`。

## 部署在 NAS 上

**群晖（Container Manager）：**

1. 打开 **Container Manager → 注册表**，搜索 `diygod/rsshub`，下载 `latest` 标签。
2. 在**映像**里选中它，点**运行**。
3. 勾选**启用自动重新启动**；在**端口设置**里把本地端口 `1200` 映射到容器端口 `1200`。
4. 完成向导，容器会自动启动。

**威联通（Container Station）：** 用 `diygod/rsshub` 镜像创建容器，**重启策略**选「总是」，添加端口映射 `1200 → 1200`。

ShibaPost 里填：`http://<NAS 的地址>:1200`，比如 `http://192.168.1.20:1200`。

## 部署在云服务器上

用 Docker Compose，并加上 Redis 做缓存，热门的订阅不用一遍遍重新抓。新建 `docker-compose.yml`：

```yaml
services:
  rsshub:
    image: diygod/rsshub
    restart: unless-stopped
    ports:
      - "1200:1200"
    environment:
      NODE_ENV: production
      CACHE_TYPE: redis
      REDIS_URL: redis://redis:6379/
      # 不设的话，知道地址的人都能用你的服务器。换成一串很长的随机字符。
      ACCESS_KEY: change-this-to-a-long-random-string
    depends_on:
      - redis

  redis:
    image: redis:alpine
    restart: unless-stopped
    volumes:
      - redis-data:/data

volumes:
  redis-data:
```

然后运行：

```bash
docker compose up -d
```

要用 HTTPS，在前面加一个反向代理。用 [Caddy](https://caddyserver.com/) 的话，`Caddyfile` 写两行就够了，证书 Caddy 会自己申请：

```
rsshub.你的域名
reverse_proxy localhost:1200
```

ShibaPost 里填：`https://rsshub.你的域名`，**访问密钥**填和 `ACCESS_KEY` 一样的值。

## 在 ShibaPost 里填写

1. 打开**设置 → 订阅 → RSSHub**（**添加订阅 → RSSHub** 里也有同样的框）。
2. 填实例地址：只写协议、主机和端口，比如 `http://192.168.1.20:1200`。
3. 如果设了 `ACCESS_KEY`，把它填进**访问密钥**。ShibaPost 会在 RSSHub 的订阅地址后面加上 `?key=`。
4. 点**测试连接**。ShibaPost 会向实例请求 RSSHub 自带的测试订阅（不会访问别的网站），告诉你能不能用。

地址和密钥存在 ShibaPost 的同步设置里，你的所有设备都会用上。这也意味着它们保存在数据目录里，能读到这个目录的人也能用你的实例。

## 需要额外配置的路由

有些路由在 ShibaPost 里标着**需实例配置**：它们需要网站的 Cookie、令牌或 API 密钥，以环境变量的形式交给 RSSHub
（比如某个社交平台的登录 Cookie）。每条路由需要哪个变量，见 RSSHub 的[配置文档](https://docs.rsshub.app/zh/deploy/config)。
把它们加在 Compose 文件的 `environment:` 下面（用 `docker run` 的话是 `-e 名字=值`），然后重启容器。

反爬很严的网站（标着**有反爬**）即使用自己的实例，也可能时不时失败。

## 更新

网站经常改版，RSSHub 也更新得很勤，建议每隔几周更新一次：

```bash
docker pull diygod/rsshub
docker rm -f rsshub
docker run -d --name rsshub --restart unless-stopped -p 1200:1200 diygod/rsshub
```

用 Compose 的话：`docker compose pull && docker compose up -d`。NAS 上重新下载镜像，再重建容器。

## 排查问题

**测试连接**会提示以下几种情况：

| 提示 | 怎么办 |
| --- | --- |
| 可以用 | 不用做什么，直接订阅。 |
| 连不上 | 检查地址和端口；用 `docker ps` 看容器是不是在运行；如果部署在 NAS 或电脑上，确认在同一个网络里。 |
| 开了防机器人验证 | 填的是 `rsshub.app` 这类受保护的实例，换成自己的。 |
| 拒绝了访问，访问密钥 | 把实例的 `ACCESS_KEY` 原样填进**访问密钥**。 |
| 返回的不是 RSSHub 的订阅源 | 地址里多写了路径，或者指向了别的服务。只写主机和端口。 |
| 出错了（5xx） | 实例本身有问题，用 `docker logs rsshub` 看日志。 |

测试能通过、但某个订阅失败时（ShibaPost 提示这个路由没取到内容，HTTP 503），多半是这条路由需要额外配置，或者那个网站暂时在拦截请求，
实例的日志（`docker logs rsshub`）里通常能看出是哪种。最常见的是 YouTube：RSSHub 的 YouTube 路由要设置 `YOUTUBE_KEY`
（Google 的 YouTube Data API 密钥）。其实 YouTube 不用 RSSHub 也能订阅：在 RSSHub 表单里点**直接订阅**，
或者在**添加订阅 → 网址**里粘贴频道地址。
