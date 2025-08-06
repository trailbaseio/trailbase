import { adminFetch } from "@/lib/fetch";

import type { ListJobsResponse } from "@bindings/ListJobsResponse";
import type { RunJobRequest } from "@bindings/RunJobRequest";
import type { RunJobResponse } from "@bindings/RunJobResponse";

export async function listJobs(): Promise<ListJobsResponse> {
  const response = await adminFetch("/jobs", {
    method: "GET",
  });
  return await response.json();
}

export async function runJob(request: RunJobRequest): Promise<RunJobResponse> {
  const response = await adminFetch("/job/run", {
    method: "POST",
    body: JSON.stringify(request),
  });
  return await response.json();
}
