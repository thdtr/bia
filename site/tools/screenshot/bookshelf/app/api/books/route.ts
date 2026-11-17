import { NextResponse } from "next/server";
import { db } from "@/lib/db";

export async function GET() {
  const books = await db.book.findMany({ orderBy: { title: "asc" } });
  return NextResponse.json(books);
}
