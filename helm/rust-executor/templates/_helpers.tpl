{{/*
Expand the name of the chart.
*/}}
{{- define "rust-executor.name" -}}
{{- default .Chart.Name .Values.nameOverride | trunc 63 | trimSuffix "-" }}
{{- end }}

{{/*
Create a default fully qualified app name.
*/}}
{{- define "rust-executor.fullname" -}}
{{- if .Values.fullnameOverride }}
{{- .Values.fullnameOverride | trunc 63 | trimSuffix "-" }}
{{- else }}
{{- $name := default .Chart.Name .Values.nameOverride }}
{{- if contains $name .Release.Name }}
{{- .Release.Name | trunc 63 | trimSuffix "-" }}
{{- else }}
{{- printf "%s-%s" .Release.Name $name | trunc 63 | trimSuffix "-" }}
{{- end }}
{{- end }}
{{- end }}

{{/*
Create chart name and version as used by the chart label.
*/}}
{{- define "rust-executor.chart" -}}
{{- printf "%s-%s" .Chart.Name .Chart.Version | replace "+" "_" | trunc 63 | trimSuffix "-" }}
{{- end }}

{{/*
Common labels
*/}}
{{- define "rust-executor.labels" -}}
helm.sh/chart: {{ include "rust-executor.chart" . }}
{{ include "rust-executor.selectorLabels" . }}
{{- if .Chart.AppVersion }}
app.kubernetes.io/version: {{ .Chart.AppVersion | quote }}
{{- end }}
app.kubernetes.io/managed-by: {{ .Release.Service }}
{{- end }}

{{/*
Selector labels
*/}}
{{- define "rust-executor.selectorLabels" -}}
app.kubernetes.io/name: {{ include "rust-executor.name" . }}
app.kubernetes.io/instance: {{ .Release.Name }}
{{- end }}

{{/*
Executor labels
*/}}
{{- define "rust-executor.executor.labels" -}}
{{ include "rust-executor.labels" . }}
app.kubernetes.io/component: executor
{{- end }}

{{/*
Executor selector labels
*/}}
{{- define "rust-executor.executor.selectorLabels" -}}
{{ include "rust-executor.selectorLabels" . }}
app.kubernetes.io/component: executor
{{- end }}

{{/*
Create the name of the service account to use
*/}}
{{- define "rust-executor.serviceAccountName" -}}
{{- if .Values.serviceAccount.create }}
{{- default (include "rust-executor.fullname" .) .Values.serviceAccount.name }}
{{- else }}
{{- default "default" .Values.serviceAccount.name }}
{{- end }}
{{- end }}

{{/*
Database host
*/}}
{{- define "rust-executor.database.host" -}}
{{- if .Values.postgresql.enabled }}
{{- printf "%s-postgresql" (include "rust-executor.fullname" .) }}
{{- else }}
{{- .Values.externalDatabase.host }}
{{- end }}
{{- end }}

{{/*
Database port
*/}}
{{- define "rust-executor.database.port" -}}
{{- if .Values.postgresql.enabled }}
5432
{{- else }}
{{- .Values.externalDatabase.port }}
{{- end }}
{{- end }}

{{/*
Database name
*/}}
{{- define "rust-executor.database.name" -}}
{{- if .Values.postgresql.enabled }}
{{- .Values.postgresql.auth.database }}
{{- else }}
{{- .Values.externalDatabase.database }}
{{- end }}
{{- end }}

{{/*
Database username
*/}}
{{- define "rust-executor.database.username" -}}
{{- if .Values.postgresql.enabled }}
{{- .Values.postgresql.auth.username }}
{{- else }}
{{- .Values.externalDatabase.username }}
{{- end }}
{{- end }}

{{/*
Database password secret name
*/}}
{{- define "rust-executor.database.secretName" -}}
{{- if .Values.postgresql.enabled }}
{{- printf "%s-postgresql" (include "rust-executor.fullname" .) }}
{{- else }}
{{- .Values.externalDatabase.existingSecret }}
{{- end }}
{{- end }}

{{/*
Database password secret key
*/}}
{{- define "rust-executor.database.secretKey" -}}
{{- if .Values.postgresql.enabled }}
password
{{- else }}
{{- .Values.externalDatabase.existingSecretPasswordKey }}
{{- end }}
{{- end }}

{{/*
Database connection URL for Rust executor
The Rust executor expects DATABASE_URL in PostgreSQL format
*/}}
{{- define "rust-executor.database.url" -}}
{{- printf "postgresql://%s@%s:%s/%s" (include "rust-executor.database.username" . | trim) (include "rust-executor.database.host" . | trim) (include "rust-executor.database.port" . | trim) (include "rust-executor.database.name" . | trim) }}
{{- end }}
