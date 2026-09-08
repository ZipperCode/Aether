# Aether v0.7.31 发布证据

## Tag 与 CI

- 稳定应用版本：v0.7.31，发布前latest为v0.7.30；独立Tunnel tag未改。
- annotated tag对象：188ace217a49886f7542b3d5aaf88faac8d0e1a9。
- tag目标：74abb40cfd8f4c9df93ce657c8bbcbf9996acda6，本地及远端解引用一致。
- 目标已有Rust CI34220941447，18/18作业成功。本次未重复编译CI；master4f7e32c2c与目标仅.trellis记录不同，产品代码一致。

## 发布结果

- Release：https://github.com/ZipperCode/Aether/releases/tag/v0.7.31。
- Release Aether：https://github.com/ZipperCode/Aether/actions/runs/34230430079。
- run headSha与tag目标一致，8/8作业全部成功；2026-09-08 13:11:14Z启动，13:29:06Z最后作业结束，17m52s。
- GitHub Release发布于13:29:04Z，非draft、非prerelease，已标latest。
- amd64构建16m28s、arm64构建15m06s；前端48s、VSIX46s、tarballs23s、多架构镜像57s、Release资产12s。作业并行，不能将时长相加作为等待时间。

## 发布资产

| 资产 | 字节 | SHA-256 |
| --- | ---: | --- |
| aether-v0.7.31-linux-amd64.tar.gz | 44306868 | aa5e1449577a08ee7c74683f409e59be3e2f5251a2c12219d3de5e779eab8ec8 |
| aether-v0.7.31-linux-arm64.tar.gz | 40531905 | 442856f378b43aa33ebc193176e71f0790a07a3063f46c2575986fce45d895cc |
| aether-vscodex-0.4.0.vsix | 328733 | c74093833e5bf4df79ae87441d824ab12573e4fb358a46ece2d812cdcdf40c8a |
| AETHER_RELEASE_PROVENANCE.sigstore.json | 11290 | e3fef552644107c97becb589b53124dc137cf3c902c974df833f89131c042681 |
| install.sh | 92886 | bba12507fc48e9533fa8c777cfb8fe8eb5658fb833252c3a5a3a4679cb242572 |
| SHA256SUMS | 200 | cc03a5528f67add0669bdfbbea2d72764c421c93a31c62e04dcd61c09869706b |

全部6项state=uploaded且非空。下载后四项小资产并逐个本地Get-FileHash匹配GitHub digest；两份tar.gz未完整下载，使用SHA256SUMS、GitHub服务端digest及已验证provenance subject的同一摘要交叉核验，不声称已本地安装/运行tar包。

## 来源与镜像

- `gh attestation verify install.sh --repo ZipperCode/Aether --signer-workflow ZipperCode/Aether/.github/workflows/release.yml --bundle AETHER_RELEASE_PROVENANCE.sigstore.json` exit0，JSON验证输出含两tar.gz、install.sh和SHA256SUMS的subject摘要。
- 签名statement来源：git+https://github.com/ZipperCode/Aether@refs/tags/v0.7.31；gitCommit=74abb40cfd8f4c9df93ce657c8bbcbf9996acda6；builder为release.yml@refs/tags/v0.7.31。
- 安装脚本默认SOURCE_REF、VERSION均为v0.7.31；仅读取校验，没有执行安装。
- `docker buildx imagetools inspect ghcr.io/zippercode/aether:0.7.31`成功。
- OCI index digest：sha256:8c46830568ae69725b47b1022787942382e2cee4d54e25640f5d4aac048de082。
- linux/amd64 manifest：sha256:7150305a9950e6ef9648d48c3ea91bd7c6f6609b3485a0e5a14591be616964ae。
- linux/arm64 manifest：sha256:f8d78d1fb034125d9a0ab0c2c0c88bef07f7c091dcee11d4326c1cec683996a2。
- 另两份unknown/unknown manifest被明确标为attestation-manifest，不是缺失运行平台。未pull镜像或启动容器。

## 收尾边界

本次只新增tag并由已有工作流发布，不移动/覆盖旧tag，不改权限、secret、产品代码、现有数据库或部署。发布监测进程已退出；任务下载的四个小资产和独占临时目录在归档前清理，远端资产保留可重新下载。Node Action运行时弃用提示未阻塞任何作业，本次未扩展修改工作流。
