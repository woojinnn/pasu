# policy-server 배포 가이드 (deploy-guideline)

> 쿠버네티스·GCP를 처음 만지는 사람도 **복붙으로 따라 할 수 있게** 쓴 가이드입니다.
> 다른 GCP 계정/프로젝트에서도 이 문서만 보고 처음부터 끝까지 배포할 수 있어요.
> 그리고 **안 쓸 때 돈 안 나가게 끄는 법**(맨 아래 "비용/끄기")도 같이 있습니다.

---

## 0. 이게 뭐예요? (30초 요약)

`policy-server`는 **API 서버 + 백그라운드 동기화 워커** 두 개로 이루어진 Rust 프로그램입니다.
데이터는 **PostgreSQL**(영구 저장)과 **Redis**(실시간 이벤트 전파)에 둡니다.
이걸 구글 클라우드의 쿠버네티스(**GKE Autopilot**) 위에 올립니다.

최종적으로 얻는 것:
- `https://<당신의도메인>` 으로 접속되는 API (HTTPS, 자동 인증서)
- 구글 로그인(OAuth)
- 자동 확장/복구되는 컨테이너 2~3개

### 핵심 용어 (초보용)
| 용어 | 한 줄 설명 |
|---|---|
| **컨테이너 이미지** | 프로그램을 통째로 담은 "실행 파일 꾸러미". `docker build`로 만든다. |
| **Artifact Registry (AR)** | 구글의 이미지 저장소. 만든 이미지를 여기에 올린다(push). |
| **GKE / 클러스터** | 컨테이너를 굴려주는 구글의 쿠버네티스. "컴퓨터 여러 대 묶음"이라 보면 됨. |
| **Pod (파드)** | 클러스터에서 실제로 도는 컨테이너 1개(이상). |
| **Cloud SQL** | 구글이 관리해주는 PostgreSQL. |
| **Memorystore** | 구글이 관리해주는 Redis. |
| **Terraform** | "클라우드 자원을 코드로 만든다". 한 번 `apply`하면 VPC·DB·클러스터가 다 생김. |
| **Helm** | "쿠버네티스 배포 템플릿". `helm install`로 앱을 클러스터에 올림. |
| **Ingress** | 바깥 인터넷 → 클러스터 안 API로 들어오는 입구(HTTPS 처리). |

### 흐름 한눈에
```
[1] 도구 설치 + GCP 로그인
        ↓
[2] Terraform apply  →  VPC · Cloud SQL · Redis · GKE · Artifact Registry 생성 (~25분)
        ↓
[3] docker build + push  →  이미지를 Artifact Registry에 올림
        ↓
[4] kubectl 시크릿 + helm install  →  앱이 클러스터에서 돌기 시작
        ↓
[5] (선택) 도메인 + HTTPS + 구글 로그인 붙이기
        ↓
[6] 확인 → 끝!  (안 쓸 땐 "비용/끄기"로 내림)
```

---

## 1. ⚙️ 내 프로젝트에 맞게 바꿀 값 (먼저 체크!)

다른 계정/프로젝트로 옮길 때 **여기 적힌 값만** 바꾸면 됩니다. (현재 값은 예시)

| 무엇 | 어디 파일 | 현재 값 → 바꿀 값 |
|---|---|---|
| GCP 프로젝트 ID | `server/deploy/terraform/terraform.tfvars` | `policy-engine-498313` → **당신 프로젝트 ID** |
| 리전(지역) | 같은 파일 | `asia-northeast3`(서울) → 원하면 변경 |
| Terraform 상태 버킷 | `server/deploy/terraform/backend.tf` | `policy-engine-498313-pasu-tfstate` → **`<당신프로젝트>-pasu-tfstate`** (전 세계에서 유일해야 함) |
| 이미지 경로 | `server/deploy/helm/policy-server/values-gke.yaml`, `values-m3.yaml` | `asia-northeast3-docker.pkg.dev/policy-engine-498313/...` → 당신 리전/프로젝트로 |
| 도메인(HTTPS용) | `values-m3.yaml`의 `ingress.host` | `pasu-policy.duckdns.org` → 당신 도메인 |
| CI/CD 설정 | `.github/workflows/policy-server-deploy.yml`의 `env:` | 위와 동일하게 |

> 💡 프로젝트 ID는 [console.cloud.google.com](https://console.cloud.google.com) 상단 프로젝트 선택기에서 확인.

---

## 2. 사전 준비물 (도구 설치)

macOS 기준 (Homebrew):
```bash
brew install --cask google-cloud-sdk   # gcloud (구글 클라우드 CLI)
brew install terraform                  # 인프라 코드
brew install helm                       # 쿠버네티스 배포 도구
brew install kubernetes-cli             # kubectl (없으면)
# Docker Desktop은 따로 설치: https://www.docker.com/products/docker-desktop/
```
GKE에 kubectl로 접속하려면 **인증 플러그인**이 필요합니다:
```bash
gcloud components install gke-gcloud-auth-plugin
# Homebrew라 위가 막히면, 보통 SDK에 같이 들어 있으니 PATH에 심볼릭 링크:
#   PLUGIN=$(find /opt/homebrew -name gke-gcloud-auth-plugin -type f | head -1)
#   ln -sf "$PLUGIN" /opt/homebrew/bin/gke-gcloud-auth-plugin
gke-gcloud-auth-plugin --version   # 버전이 나오면 OK
```
필요한 계정:
- **GCP 계정** + **프로젝트 1개** + **결제(billing) 연결** (GKE/Cloud SQL는 결제 필수)

---

## 2.5 로컬에서 테스트하기 (클라우드 없이)

클라우드에 올리기 전에 **내 컴퓨터에서** policy-server를 돌려보는 가장 빠른 길. (Docker만 있으면 됨)

### 방법 A — `cargo run` (가장 가벼움, 추천)
**1) Postgres + Redis를 Docker로**
```bash
docker run -d --name pasu-pg    -p 5432:5432 \
  -e POSTGRES_USER=pasu -e POSTGRES_PASSWORD=pasu -e POSTGRES_DB=pasu postgres:16
docker run -d --name pasu-redis -p 6379:6379 redis:7
```

**2) `.env.local` 만들기** (예시 복사 후 채우기)
```bash
cp .env.local.example .env.local
```
`.env.local`에서 최소 이것만 채우면 됨:
```
DATABASE_URL=postgres://pasu:pasu@127.0.0.1:5432/pasu
REDIS_URL=redis://127.0.0.1:6379
JWT_SECRET=<`openssl rand -hex 32` 결과>
# 구글 로그인까지 테스트 안 하면 GOOGLE_* 는 placeholder여도 서버는 뜸
```

**3) 서버 실행**
```bash
scripts/start-policy-server.sh local
```
- `RUN_MIGRATIONS_ON_STARTUP` 기본 **true** → 뜰 때 **마이그레이션(0001 + 0002_market)을 로컬 DB에 자동 적용**(별도 migrate 불필요).
- `REQUIRE_SYNC_CONFIG` 기본 **false** → 로컬에선 sync 설정 없어도 OK.

**4) 확인**
```bash
curl http://127.0.0.1:8788/health             # ok
curl http://127.0.0.1:8788/readyz             # 200 (DB+Redis 연결되면)
curl -i http://127.0.0.1:8788/market/listings # 401 (마켓=OAuth, 무토큰이라 정상)
```

**5) (선택) 동기화 워커도** — 다른 터미널에서:
```bash
set -a; source .env.local; set +a
cargo run -p policy-server --bin sync_worker
```

**6) 정리**: `docker rm -f pasu-pg pasu-redis`

### 테스트 스위트 실행
```bash
docker run -d --name pasu-pg-test -p 5433:5432 \
  -e POSTGRES_USER=pasu -e POSTGRES_PASSWORD=pasu -e POSTGRES_DB=pasu_test postgres:16
TEST_DATABASE_URL=postgres://pasu:pasu@127.0.0.1:5433/pasu_test \
  cargo test -p policy-server -p policy-db -p policy-sync
```

### 방법 B — minikube (프로덕션에 가깝게, 선택/고급)
`deploy-local/`에 minikube용 매니페스트(postgres/redis/secret/values-local)+한국어 README가 있습니다. 흐름: `minikube start` → 이미지 빌드 후 `minikube image load` → postgres/redis 적용 → `create-secret.sh` → `helm install -f deploy-local/values-local.yaml` → `kubectl port-forward`.
> ⚠️ `deploy-local/`는 `.gitignore`에 등록된 **로컬 학습용 스크래치**라 새로 clone하면 없습니다. 보통 방법 A로 충분하고, minikube 풀셋업이 필요하면 커밋해드릴 수 있어요.

---

## 3. GCP 1회 셋업

```bash
# (1) 로그인 — 브라우저가 열림
gcloud auth login
gcloud auth application-default login   # ★ Terraform이 쓰는 인증(ADC)

# (2) 프로젝트 지정 (당신 프로젝트 ID로)
export PROJECT_ID=policy-engine-498313
export REGION=asia-northeast3
gcloud config set project $PROJECT_ID
gcloud auth application-default set-quota-project $PROJECT_ID

# (3) 필요한 API 켜기 (한 번만)
gcloud services enable \
  compute.googleapis.com container.googleapis.com sqladmin.googleapis.com \
  redis.googleapis.com servicenetworking.googleapis.com artifactregistry.googleapis.com \
  iam.googleapis.com cloudresourcemanager.googleapis.com --project=$PROJECT_ID

# (4) Terraform 상태를 저장할 버킷 (이름은 전 세계 유일해야 함)
gcloud storage buckets create gs://${PROJECT_ID}-pasu-tfstate \
  --location=$REGION --uniform-bucket-level-access --project=$PROJECT_ID
gcloud storage buckets update gs://${PROJECT_ID}-pasu-tfstate --versioning
```
> ⚠️ API 켠 직후 1~2분은 "서비스 네트워킹"이 덜 퍼져서 첫 `apply`가 한 번 실패할 수 있어요. 그땐 **그냥 `terraform apply` 다시** 하면 됩니다(아래 4단계 참고).

---

## 4. Terraform으로 인프라 만들기

```bash
cd crates/policy-server/server/deploy/terraform

# (1) terraform.tfvars 와 backend.tf 를 당신 값으로 수정 (1번 표 참고)
#     terraform.tfvars:  project_id = "<당신 프로젝트>"
#     backend.tf:        bucket     = "<당신프로젝트>-pasu-tfstate"

terraform init        # 백엔드(버킷) 연결 + 플러그인 다운로드
terraform plan -out=tf.plan
terraform apply tf.plan     # ⏰ 15~25분 (Cloud SQL·GKE가 오래 걸림)
```
만들어지는 것(총 13개): VPC + 사설 연결, Artifact Registry, GKE Autopilot, Cloud SQL(사설 IP), Memorystore(Redis), IAM.

> 첫 apply가 `servicenetworking ... Error code 16`로 실패하면 → 정상 범주. **`terraform apply tf.plan` 한 번 더** (또는 `terraform plan` 다시 뽑고 apply).
> Cloud SQL가 `Invalid Tier ... ENTERPRISE_PLUS`라고 하면 → `cloudsql.tf`에 `edition = "ENTERPRISE"`가 있는지 확인(이미 들어가 있음).

끝나면 **접속 정보**를 확인:
```bash
terraform output                       # 전체
terraform output -raw cloudsql_private_ip   # DB 사설 IP
terraform output -raw redis_url             # redis://...:6379
terraform output -raw ingress_ip            # (HTTPS용) 글로벌 고정 IP
```

---

## 5. 이미지 빌드 + 푸시 (수동)

> CI/CD를 쓰면 이건 자동(8번 참고). 처음 1회는 수동으로 합니다.

```bash
cd <레포 최상위>
gcloud auth configure-docker ${REGION}-docker.pkg.dev --quiet
TAG=$(git rev-parse --short HEAD)
IMG="${REGION}-docker.pkg.dev/${PROJECT_ID}/pasu/pasu-policy-server:${TAG}"

# ⚠️ Mac(애플 실리콘)이면 --platform linux/amd64 필수! (GKE는 amd64)
docker buildx build --platform linux/amd64 \
  -t "$IMG" -f crates/policy-server/server/Dockerfile --push .
```

---

## 6. 배포 (임시 HTTP — 도메인 없이 먼저 확인)

```bash
# (1) 클러스터에 kubectl 연결
gcloud container clusters get-credentials pasu-autopilot --region $REGION --project $PROJECT_ID

# (2) 네임스페이스 + 시크릿 생성
#     DB/Redis 주소는 Terraform output에서 가져옴. GOOGLE_* 는 임시 placeholder로도 OK(로그인 안 쓸 때).
cd crates/policy-server/server/deploy/terraform
DB_URL=$(terraform output -raw database_url)
REDIS_URL=$(terraform output -raw redis_url)
cd <레포 최상위>

kubectl create namespace pasu
kubectl -n pasu create secret generic policy-server-secrets \
  --from-literal=DATABASE_URL="$DB_URL" \
  --from-literal=REDIS_URL="$REDIS_URL" \
  --from-literal=JWT_SECRET="$(openssl rand -hex 32)" \
  --from-literal=GOOGLE_CLIENT_ID="placeholder" \
  --from-literal=GOOGLE_CLIENT_SECRET="placeholder" \
  --from-literal=GOOGLE_REDIRECT_URI="http://placeholder/auth/google/callback"

# (3) Helm으로 배포 (임시 LoadBalancer = 공인 IP + HTTP)
TAG=$(git rev-parse --short HEAD)
helm upgrade --install pasu crates/policy-server/server/deploy/helm/policy-server \
  -n pasu -f crates/policy-server/server/deploy/helm/policy-server/values-gke.yaml \
  --set image.tag="$TAG"

# (4) 외부 IP 확인 → 접속
kubectl -n pasu get svc pasu-policy-server-api -w   # EXTERNAL-IP 나오면 Ctrl-C
LB=$(kubectl -n pasu get svc pasu-policy-server-api -o jsonpath='{.status.loadBalancer.ingress[0].ip}')
curl http://$LB/health   # ok 나오면 성공
```
> 이 IP는 **재배포하면 바뀝니다**(임시용). 고정 주소 + HTTPS + 로그인은 7번에서.

---

## 7. (선택) 고정 HTTPS 도메인 + 구글 로그인

도메인이 필요합니다. 돈 안 쓰려면 **무료 DuckDNS**를 추천:
1. [duckdns.org](https://duckdns.org) → 구글 로그인 → 서브도메인 1개 생성 (예: `myapp` → `myapp.duckdns.org`) → **token** 복사.

```bash
export DOMAIN=myapp.duckdns.org
export DUCKDNS_SUB=myapp
export DUCKDNS_TOKEN=<복사한 토큰>

# (1) 고정 IP는 Terraform이 이미 만듦(ingress_ip). DuckDNS를 그 IP로 가리키게:
INGRESS_IP=$(terraform -chdir=crates/policy-server/server/deploy/terraform output -raw ingress_ip)
curl "https://www.duckdns.org/update?domains=${DUCKDNS_SUB}&token=${DUCKDNS_TOKEN}&ip=${INGRESS_IP}"   # OK
dig +short $DOMAIN    # INGRESS_IP 가 나오면 OK

# (2) values-m3.yaml 의 ingress.host 를 $DOMAIN 으로 수정 (1번 표)

# (3) 구글 OAuth 설정 (콘솔에서 수동, 한 번만):
#   APIs & Services → Credentials → OAuth 2.0 Client(웹) 편집
#     Authorized redirect URIs 에 추가:  https://<DOMAIN>/auth/google/callback
#     Authorized JavaScript origins 에 추가:  https://<DOMAIN>
#   OAuth consent screen → "Testing" 모드 + 본인 이메일을 Test users 에 추가
#   그리고 Client ID / Secret 을 복사해 둠

# (4) 시크릿을 실제 값으로 갱신 (로그인용)
kubectl -n pasu create secret generic policy-server-secrets \
  --from-literal=DATABASE_URL="$DB_URL" --from-literal=REDIS_URL="$REDIS_URL" \
  --from-literal=JWT_SECRET="$(kubectl -n pasu get secret policy-server-secrets -o jsonpath='{.data.JWT_SECRET}' | base64 -d)" \
  --from-literal=GOOGLE_CLIENT_ID="<당신 client id>" \
  --from-literal=GOOGLE_CLIENT_SECRET="<당신 client secret>" \
  --from-literal=GOOGLE_REDIRECT_URI="https://${DOMAIN}/auth/google/callback" \
  --dry-run=client -o yaml | kubectl apply -f -

# (5) HTTPS용 values-m3 로 재배포
TAG=$(git rev-parse --short HEAD)
helm upgrade --install pasu crates/policy-server/server/deploy/helm/policy-server \
  -n pasu -f crates/policy-server/server/deploy/helm/policy-server/values-m3.yaml \
  --set image.tag="$TAG"
kubectl -n pasu rollout restart deploy/pasu-policy-server-api   # 새 시크릿 반영

# (6) 인증서가 발급될 때까지 대기 (15~60분). Active 되면 HTTPS 켜짐.
kubectl -n pasu get managedcertificate -w   # certificateStatus 가 Active 되면 Ctrl-C
curl https://$DOMAIN/health   # 200 이면 성공
```
> ⚠️ 인증서가 `FAILED_NOT_VISIBLE`면 = DNS가 아직 그 IP를 안 가리키는 것. `dig +short $DOMAIN` 다시 확인.
> ⚠️ Ingress 주소가 안 뜨면(`<pending>` 지속): 이 클러스터엔 `gce` IngressClass가 없어서 `ingressClassName` 대신 **`kubernetes.io/ingress.class: "gce"` 어노테이션**을 씁니다(이미 차트에 반영됨).

### 로그인 동작 확인
브라우저로 `https://<DOMAIN>/auth/google` → 구글 로그인 → 콜백.
> 로그인 성공 후 `/auth/callback#access_token=...` 에서 **401 JSON**이 떠도 정상입니다. 토큰을 받아갈 **프론트엔드(대시보드)가 아직 없을 뿐**, 로그인 자체는 성공(주소창 `#` 뒤가 발급된 토큰). DB로 확인하려면 `users` 테이블에 내 구글 이메일이 들어왔는지 보면 됨.

---

## 8. 🤖 CI/CD — main에 머지하면 자동 빌드+배포

`.github/workflows/policy-server-deploy.yml` 가 **main 푸시 시** 이미지 빌드+푸시+helm 배포를 자동으로 합니다.
한 번만 아래 인증을 셋업하면 됩니다 (키 파일 없이 안전한 **Workload Identity Federation** 방식).

```bash
export PROJECT_ID=<당신 프로젝트>
export REPO="<GitHub오너>/<레포이름>"     # 예: woojinnn/pasu

# WIF에 필요한 API
gcloud services enable iamcredentials.googleapis.com sts.googleapis.com --project=$PROJECT_ID

# (1) 배포용 서비스 계정
gcloud iam service-accounts create pasu-deployer --display-name="pasu CI deployer" --project=$PROJECT_ID
DEPLOY_SA="pasu-deployer@${PROJECT_ID}.iam.gserviceaccount.com"

# (2) 권한: 이미지 push + GKE 배포
gcloud projects add-iam-policy-binding $PROJECT_ID --member="serviceAccount:$DEPLOY_SA" --role="roles/artifactregistry.writer"
gcloud projects add-iam-policy-binding $PROJECT_ID --member="serviceAccount:$DEPLOY_SA" --role="roles/container.developer"

# (3) GitHub Actions가 키 없이 위 SA를 빌려쓰게 하는 "신뢰 다리"(WIF)
gcloud iam workload-identity-pools create github-pool --location=global --project=$PROJECT_ID --display-name="GitHub Actions"
POOL=$(gcloud iam workload-identity-pools describe github-pool --location=global --project=$PROJECT_ID --format="value(name)")
gcloud iam workload-identity-pools providers create-oidc github-provider \
  --location=global --workload-identity-pool=github-pool --project=$PROJECT_ID \
  --display-name="GitHub provider" \
  --attribute-mapping="google.subject=assertion.sub,attribute.repository=assertion.repository" \
  --attribute-condition="assertion.repository=='${REPO}'" \
  --issuer-uri="https://token.actions.githubusercontent.com"

# (4) 우리 레포만 그 SA를 빌릴 수 있게 묶기
gcloud iam service-accounts add-iam-policy-binding $DEPLOY_SA --project=$PROJECT_ID \
  --role="roles/iam.workloadIdentityUser" \
  --member="principalSet://iam.googleapis.com/${POOL}/attribute.repository/${REPO}"

# (5) GitHub 시크릿에 넣을 값 출력
echo "GCP_DEPLOY_SA = $DEPLOY_SA"
gcloud iam workload-identity-pools providers describe github-provider \
  --location=global --workload-identity-pool=github-pool --project=$PROJECT_ID --format="value(name)"
# 위 두 값을 GitHub 레포 → Settings → Secrets and variables → Actions 에 등록:
#   GCP_DEPLOY_SA      = pasu-deployer@<PROJECT>.iam.gserviceaccount.com
#   GCP_WIF_PROVIDER   = (마지막 명령 출력값, projects/.../providers/github-provider)
```
워크플로 맨 위 `env:`(PROJECT_ID/REGION/...)도 당신 값으로 바꾸세요.
이후 **main에 머지하면** 자동 배포됩니다. (Actions 탭에서 "Run workflow"로 수동 실행도 가능)

> 참고: 시크릿(`policy-server-secrets`)은 CI가 안 건드립니다 — 위 6/7번에서 만든 게 클러스터에 그대로 남아있고, CI는 이미지+helm만 갱신합니다.

---

## 8.5 무중단 배포 (zero-downtime 롤아웃)

새 버전을 배포해도 서비스가 안 끊기게 하는 설정이 들어가 있습니다 (GKE L7 Ingress 기준).

**적용된 것:**
- `RollingUpdate` **maxSurge:1 / maxUnavailable:0** — 새 pod가 Ready된 뒤에야 옛 pod를 내림
- `readinessProbe` + GCE `BackendConfig` 헬스체크 모두 **`/readyz`** — 준비된 pod에만 트래픽
- **graceful shutdown** — SIGTERM 시 진행 중 요청 + SSE 드레인
- **`preStop: sleep 15` + `terminationGracePeriodSeconds: 30`** — SIGTERM 직전 15초간 계속 serving → GCLB가 NEG에서 이 pod를 빼는 동안 새 요청이 종료 중 pod로 가지 않게 함(L7의 NEG 디레지스터 갭 = 502 원인 제거). 값은 `values.yaml`의 `api.preStopSleepSeconds` / `api.terminationGracePeriodSeconds`로 조절.
- **replicas 2 + PDB minAvailable:1**

**검증** (롤아웃 거는 동안 다른 터미널에서):
```bash
while true; do curl -s -o /dev/null -w "%{http_code} " https://pasu-policy.duckdns.org/readyz; sleep 1; done
# 전부 200이면 무중단 OK. 새 pod에 preStop 박혔는지:
kubectl -n pasu get pod -l app.kubernetes.io/component=api \
  -o jsonpath='{.items[0].spec.containers[0].lifecycle.preStop}'; echo
```

**⚠️ 주의 2가지:**
- preStop을 **처음 넣는** 롤아웃은 옛 pod에 preStop이 아직 없어 그 한 번만 짧게 502 가능(이후부터 무중단). 정상.
- **파괴적 마이그레이션**(컬럼/테이블 DROP, 타입 변경)은 롤아웃 중 구·신 pod가 같은 DB를 공유하므로 깨짐 → **expand-contract**(① 추가 마이그레이션 → ② 코드 이행 → ③ 제거) 패턴 필요. (`0002_market`처럼 `CREATE TABLE` 추가형은 안전.)

---

## 9. 💰 비용 / 끄고 켜기 (중요!)

이 스택은 **켜둔 동안 계속 과금**됩니다. 대략(서울 리전, 24시간 기준):
- GKE Autopilot(파드 2~3개) + Cloud SQL(`db-custom-1-3840`) + Memorystore(1GB) + (HTTPS면) L7 로드밸런서
- **대략 월 $100~180 (하루 $3~6)** 정도 — 정확한 건 사용량에 따라 다름. **안 쓰면 반드시 끄세요.**

### 끄기 (과금 정지)
```bash
# (1) 앱 내리기 → 로드밸런서/외부 IP도 같이 해제됨
helm uninstall pasu -n pasu
kubectl delete namespace pasu

# (2) 클라우드 자원 전부 삭제
cd crates/policy-server/server/deploy/terraform
terraform destroy        # yes 입력
```
> `deletion_protection`을 꺼둬서(`cloudsql.tf`, `gke.tf`) destroy가 깔끔히 됩니다. SQL destroy가 순서로 투덜대면 `terraform destroy` 한 번 더.
> **남는 것**(거의 무료): GCS 상태 버킷, 켜둔 API, DuckDNS 서브도메인. 그래서 다시 켤 때 빠름.
> ⚠️ `terraform destroy`는 **Cloud SQL 데이터도 삭제**합니다. 중요 데이터가 있으면 먼저 백업하세요.

### 다시 켜기
```bash
cd crates/policy-server/server/deploy/terraform && terraform apply   # ~25분
# 그 다음 6번(또는 7번) 배포 단계 반복.
# HTTPS(7번) 쓰면, 새로 생긴 ingress_ip로 DuckDNS만 다시 가리키면 됨(7-(1)).
```

### 비용 더 줄이기
- Cloud SQL 티어를 더 작게(`terraform.tfvars`의 `db_tier`를 `db-g1-small` 등)
- Memorystore를 안 쓰면 단일 파드로도 동작(다만 교차복제 이벤트는 비활성)
- 안 쓸 땐 그냥 위 "끄기"가 제일 확실

---

## 10. 🆘 자주 막히는 곳

| 증상 | 원인 / 해결 |
|---|---|
| `terraform apply` 첫 시도 `Error code 16` | servicenetworking 전파 지연 → **apply 다시** |
| Cloud SQL `Invalid Tier ... ENTERPRISE_PLUS` | `cloudsql.tf`에 `edition = "ENTERPRISE"` 필요(반영됨) |
| `kubectl`이 클러스터 접속 안 됨 | `gke-gcloud-auth-plugin` 미설치 → 2번 참고 |
| 파드 `ImagePullBackOff` | 이미지 태그 불일치 or AR 권한. 이미지 push 됐는지/태그 맞는지 확인 |
| 파드 `exec format error` | 이미지가 arm64 → Mac에서 `--platform linux/amd64`로 다시 빌드 |
| Ingress `EXTERNAL-IP`/`ADDRESS`가 계속 `<pending>` | `gce` IngressClass 없음 → `kubernetes.io/ingress.class: "gce"` 어노테이션 사용(반영됨) |
| 인증서 `FAILED_NOT_VISIBLE` | DNS가 ingress IP를 안 가리킴 → DuckDNS 재설정 + `dig`로 확인 |
| 로그인 후 401 JSON | 정상(프론트 없음). 로그인 자체는 성공 — 토큰은 주소창 `#` 뒤 |
| OAuth `redirect_uri_mismatch` | 콘솔의 redirect URI와 `GOOGLE_REDIRECT_URI`(시크릿)가 정확히 같아야 함 |
| HTTPS 200인데 백엔드 502 | 헬스체크 경로 문제 → BackendConfig가 `/readyz`로 맞춰져 있는지(반영됨) |

---

## 부록: 파일 지도

```
crates/policy-server/
├── deploy-guideline.md                    ← 이 문서
└── server/
    ├── Dockerfile                          ← 이미지 빌드 레시피
    └── deploy/
        ├── terraform/                      ← 클라우드 인프라(코드)
        │   ├── *.tf  (network, gke, cloudsql, memorystore, iam, ingress, ...)
        │   ├── terraform.tfvars            ← 프로젝트 ID/리전 (여기 수정)
        │   └── backend.tf                  ← 상태 버킷 이름 (여기 수정)
        └── helm/policy-server/             ← 쿠버네티스 배포 템플릿
            ├── values.yaml                 ← 기본값
            ├── values-gke.yaml             ← M2: 임시 HTTP(LoadBalancer)
            └── values-m3.yaml              ← M3: HTTPS 도메인(GCE Ingress) (host 수정)
.github/workflows/policy-server-deploy.yml  ← CI/CD (env 수정)
```
