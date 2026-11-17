# Running your own RSSHub

[中文版](rsshub.zh-CN.md)

[RSSHub](https://github.com/DIYgod/RSSHub) makes feeds for thousands of sites that don't have one: video
channels, social media, forums, app stores, project pages. Postflow can search its routes and fill them in
for you (**Add feed → RSSHub**), but the feeds have to come from an RSSHub server, called an *instance*.

The official public instance, `rsshub.app`, is meant for trying things out. It puts a bot check in front of
every request, so feed readers like Postflow can't use it. Running your own instance takes a few minutes and
about 130 MB of download.

> You may not need RSSHub at all. Most blogs and news sites have feeds Postflow finds by itself, and YouTube
> channels and playlists can be followed directly: paste the address in **Add feed → Address**.

- [Choose where to run it](#choose-where-to-run-it)
- [On your computer](#on-your-computer)
- [On a NAS](#on-a-nas)
- [On a cloud server](#on-a-cloud-server)
- [Connect Postflow](#connect-postflow)
- [Routes that need more setup](#routes-that-need-more-setup)
- [Updating](#updating)
- [Troubleshooting](#troubleshooting)

## Choose where to run it

| Where | Good for | Address in Postflow |
| --- | --- | --- |
| Your computer | Trying it; one computer | `http://localhost:1200` |
| A NAS or home server | Every device at home, always on | `http://<NAS address>:1200` |
| A cloud server | Every device, anywhere | `https://rsshub.your-domain.com` |

All three use the official Docker image `diygod/rsshub`. If a route is marked **Needs a browser** in
Postflow, use the `diygod/rsshub:chromium-bundled` image instead (about 400–500 MB); everything else stays
the same.

## On your computer

1. Install [Docker Desktop](https://www.docker.com/products/docker-desktop/) and start it.
2. Run:

   ```bash
   docker run -d --name rsshub --restart unless-stopped -p 1200:1200 diygod/rsshub
   ```

3. Open <http://localhost:1200> in a browser. You should see the RSSHub welcome page.

Postflow's address: `http://localhost:1200`. Feeds update only while this computer is on. To use it from
your other devices on the same network, use the computer's local IP address instead of `localhost`, for
example `http://192.168.1.10:1200`.

## On a NAS

**Synology (Container Manager):**

1. Open **Container Manager → Registry**, search for `diygod/rsshub`, and download the `latest` tag.
2. In **Image**, select it and choose **Run**.
3. Turn on **Enable auto-restart**. Under **Port Settings**, map local port `1200` to container port `1200`.
4. Finish the wizard. The container starts on its own.

**QNAP (Container Station):** create a container from the `diygod/rsshub` image, set **Restart policy** to
*Always*, and add a port mapping `1200 → 1200`.

Postflow's address: `http://<NAS address>:1200`, such as `http://192.168.1.20:1200`.

## On a cloud server

Use Docker Compose, with Redis as a cache so popular feeds aren't fetched again and again. Create
`docker-compose.yml`:

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
      # Anyone who knows the address could otherwise use your server. Pick a long random string.
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

Then:

```bash
docker compose up -d
```

For HTTPS, put a reverse proxy in front. With [Caddy](https://caddyserver.com/), a `Caddyfile` of two lines
is enough, and Caddy gets the certificate by itself:

```
rsshub.your-domain.com
reverse_proxy localhost:1200
```

Postflow's address: `https://rsshub.your-domain.com`, with the same **Access key** as `ACCESS_KEY`.

## Connect Postflow

1. Open **Settings → Feeds → RSSHub** (the same box also appears in **Add feed → RSSHub**).
2. Enter the instance address: only the scheme, host and port, such as `http://192.168.1.20:1200`.
3. If you set `ACCESS_KEY`, enter it as **Access key**. Postflow adds it to RSSHub feed addresses as `?key=`.
4. Choose **Test connection**. Postflow asks the instance for RSSHub's built-in test feed, which doesn't
   contact any other site, and tells you whether it works.

The address and key are stored in Postflow's synced settings, so every device you use picks them up. That
also means they're saved in your data folder; anyone who can read that folder can use your instance.

## Routes that need more setup

Some routes are marked **Needs setup** in Postflow. They need a cookie, token or API key for the site, given to
RSSHub as an environment variable (for example a login cookie for a social network). Which variable each route
needs is listed in RSSHub's [configuration docs](https://docs.rsshub.app/deploy/config). Add them under
`environment:` in the Compose file (or `-e NAME=value` with `docker run`) and restart the container.

Sites with strict anti-scraping (marked **Anti-scraping**) may still fail from time to time, even with your own
instance.

## Updating

RSSHub changes often as sites change. Update every few weeks:

```bash
docker pull diygod/rsshub
docker rm -f rsshub
docker run -d --name rsshub --restart unless-stopped -p 1200:1200 diygod/rsshub
```

With Compose: `docker compose pull && docker compose up -d`. On a NAS, download the image again and recreate
the container.

## Troubleshooting

What **Test connection** says, and what to do:

| Message | What to do |
| --- | --- |
| Works | Nothing. Subscribe away. |
| Can't reach | Check the address and port, that the container is running (`docker ps`), and, for a NAS or computer, that you're on the same network. |
| Bot check | You're pointing at a protected instance such as `rsshub.app`. Use your own. |
| Refused, access key | Enter the instance's `ACCESS_KEY` as **Access key**, exactly. |
| Didn't answer like RSSHub | The address has an extra path, or points at something else. Use only the host and port. |
| Error (5xx) | The instance is having trouble; look at its log with `docker logs rsshub`. |

If the test works but one feed fails (Postflow says the route returned nothing, HTTP 503), the route itself may
need setup, or the site may be blocking requests for now. The instance log (`docker logs rsshub`) usually says
which. The most common case is YouTube: RSSHub's YouTube routes need `YOUTUBE_KEY` (a Google YouTube Data API
key). YouTube doesn't need RSSHub at all, though: click **Follow directly** in the RSSHub form, or paste the
channel address under **Add feed → URL**.
