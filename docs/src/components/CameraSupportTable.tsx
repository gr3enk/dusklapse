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

type CameraModel = {
    id: string;
    name: string;
    vendor_id: string;
    vendor_name: string;
    general_support: boolean;
    test_count: number;
    issues_count: number;
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

const columns: Array<ColumnDef<typeof features, CameraModel>> = [
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
];

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
    const query = useQuery({
        queryKey: ["camera-models"],
        queryFn: () => fetch("https://api.dusklapse.com/data/camera-models").then((res) => res.json()),
    });

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
        </div>
    );
}
