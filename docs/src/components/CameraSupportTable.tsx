import React from "react";
import BrowserOnly from "@docusaurus/BrowserOnly";
import {
    columnFilteringFeature,
    createFilteredRowModel,
    filterFn_includesString,
    tableFeatures,
    useTable,
    type ColumnDef,
    type ColumnFiltersState,
} from "@tanstack/react-table";
import { useQuery } from "@tanstack/react-query";
import { cn } from "../utils";
import Providers from "../providers";
import "../css/tailwind.css";
import { ExternalLinkIcon } from "lucide-react";

type CameraModel = {
    id: string;
    name: string;
    vendor_id: string;
    vendor_name: string;
    general_support: boolean;
    test_count: number;
    issues_count: number;
};

type CameraIssue = {
    id: string;
    title: string;
    version: string;
    created_at: string;
    gh_number: number;
};

const features = tableFeatures({
    columnFilteringFeature,
    filteredRowModel: createFilteredRowModel(),
    filterFns: {
        includesString: filterFn_includesString,
    },
});

function Badge({ children, className }: { children: React.ReactNode; className?: string }) {
    return (
        <div
            className={cn(
                `bg-blue-500 tracking-tight text-white px-2 font-semibold py-1 text-xs rounded-md`,
                className,
            )}
        >
            {children}
        </div>
    );
}

export default function CameraSupportTable() {
    return (
        <div className="tw">
            <BrowserOnly>
                {() => (
                    <Providers>
                        <CameraSupportTableInner />
                    </Providers>
                )}
            </BrowserOnly>
        </div>
    );
}

function CameraSupportTableInner() {
    const [selectedCamera, setSelectedCamera] = React.useState<CameraModel | null>(null);
    const issuesTableRef = React.useRef<HTMLDivElement>(null);

    const query = useQuery({
        queryKey: ["camera-models"],
        queryFn: () => fetch("https://api.dusklapse.com/data/camera-models").then((res) => res.json()),
    });

    const issuesQuery = useQuery<CameraIssue[]>({
        queryKey: ["camera-issues", selectedCamera?.id],
        queryFn: async () => {
            const response = await fetch(
                `https://api.dusklapse.com/data/camera-issues?q=${encodeURIComponent(selectedCamera!.id)}`,
            );
            if (!response.ok) {
                throw new Error(`Failed to load camera issues (${response.status})`);
            }
            return response.json();
        },
        enabled: selectedCamera !== null,
    });

    const showIssues = React.useCallback((camera: CameraModel) => {
        setSelectedCamera(camera);
        requestAnimationFrame(() => {
            issuesTableRef.current?.scrollIntoView({ behavior: "smooth", block: "start" });
        });
    }, []);

    const columns = React.useMemo<Array<ColumnDef<typeof features, CameraModel>>>(
        () => [
            {
                accessorKey: "vendor_name",
                header: "Vendor",
            },
            {
                accessorKey: "name",
                header: "Model",
            },
            {
                header: "Support",
                cell: ({ row }) => {
                    if (!row.original.general_support) {
                        return <Badge className="bg-red-600 text-red-100">Not Supported</Badge>;
                    }
                    if (row.original.issues_count > 0) {
                        return <Badge className="bg-orange-600 text-orange-100">Supported with Issues</Badge>;
                    }
                    if (row.original.test_count === 0) {
                        return <Badge className="bg-amber-600 text-amber-100">Supported but untested</Badge>;
                    }
                    return <Badge className="bg-green-600 text-green-100">Supported</Badge>;
                },
            },
            {
                header: "Open Issues",
                cell: ({ row }) => {
                    if (row.original.issues_count === 0) {
                        return <span className="text-green-600!">None</span>;
                    }
                    return (
                        <button
                            type="button"
                            className="p-0 border-0 bg-transparent text-red-600! underline! cursor-pointer flex items-center gap-1 font-semibold"
                            aria-expanded={selectedCamera?.id === row.original.id}
                            onClick={() => showIssues(row.original)}
                        >
                            {row.original.issues_count}
                            <ExternalLinkIcon className="w-4 h-4" />
                        </button>
                    );
                },
            },
        ],
        [selectedCamera?.id, showIssues],
    );

    const [columnFilters, setColumnFilters] = React.useState<ColumnFiltersState>([]);

    const table = useTable({
        key: "camera-support-table",
        features,
        columns,
        data: query.data || [],
        onColumnFiltersChange: setColumnFilters,
        state: {
            columnFilters,
        },
    });

    return (
        <div>
            <div className="flex gap-2 justify-start">
                <input
                    type="text"
                    placeholder="Vendor"
                    className="max-w-64"
                    value={(table.getColumn("vendor_name")?.getFilterValue() as string) ?? ""}
                    onChange={(event) => table.getColumn("vendor_name")?.setFilterValue(event.target.value)}
                />
                <input
                    type="text"
                    placeholder="Model"
                    className="max-w-64"
                    value={(table.getColumn("name")?.getFilterValue() as string) ?? ""}
                    onChange={(event) => table.getColumn("name")?.setFilterValue(event.target.value)}
                />
            </div>
            <div className="py-4 flex justify-start">
                <table>
                    <thead>
                        {table.getHeaderGroups().map((headerGroup) => (
                            <tr key={headerGroup.id}>
                                {headerGroup.headers.map((header) => (
                                    <th key={header.id}>
                                        {header.isPlaceholder ? null : <table.FlexRender header={header} />}
                                    </th>
                                ))}
                            </tr>
                        ))}
                    </thead>
                    <tbody>
                        {table.getRowModel().rows.map((row) => (
                            <tr key={row.id}>
                                {row.getAllCells().map((cell) => (
                                    <td key={cell.id}>
                                        <table.FlexRender cell={cell} />
                                    </td>
                                ))}
                            </tr>
                        ))}
                    </tbody>
                </table>
            </div>
            {selectedCamera && (
                <div ref={issuesTableRef} className="scroll-mt-4 py-4">
                    <h2>
                        Open issues for {selectedCamera.vendor_name} {selectedCamera.name}
                    </h2>
                    {issuesQuery.isPending && <p>Loading issues…</p>}
                    {issuesQuery.isError && <p role="alert">The issues could not be loaded. Please try again.</p>}
                    {issuesQuery.isSuccess && (
                        <table>
                            <thead>
                                <tr>
                                    <th>Issue</th>
                                    <th>Version</th>
                                    <th>Opened</th>
                                </tr>
                            </thead>
                            <tbody>
                                {issuesQuery.data.map((issue) => (
                                    <tr key={issue.id}>
                                        <td>
                                            <a href={`https://github.com/gr3enk/dusklapse/issues/${issue.gh_number}`}>
                                                #{issue.gh_number}: {issue.title}
                                            </a>
                                        </td>
                                        <td>{issue.version || "Unknown"}</td>
                                        <td>{new Date(issue.created_at).toLocaleDateString()}</td>
                                    </tr>
                                ))}
                            </tbody>
                        </table>
                    )}
                </div>
            )}
        </div>
    );
}
