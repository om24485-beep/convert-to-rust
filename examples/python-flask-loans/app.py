"""Tiny loan-origination API used as a c2r example."""
from flask import Flask, jsonify, request

from loans.models import LoanApplication, LoanStatus
from loans.service import LoanService

app = Flask(__name__)
service = LoanService()


@app.route("/health")
def health():
    return jsonify({"status": "ok"})


@app.route("/loans", methods=["POST"])
def create_loan():
    payload = request.get_json()
    application = LoanApplication(**payload)
    decision = service.evaluate(application)
    return jsonify(decision), 201


@app.route("/loans/<int:loan_id>", methods=["GET"])
def get_loan(loan_id: int):
    loan = service.find(loan_id)
    if loan is None:
        return jsonify({"error": "not found"}), 404
    return jsonify(loan)


@app.route("/loans/<int:loan_id>/schedule")
def repayment_schedule(loan_id: int):
    return jsonify(service.schedule(loan_id))


@app.route("/loans/<int:loan_id>/status", methods=["PUT"])
def update_status(loan_id: int):
    status = LoanStatus(request.get_json()["status"])
    return jsonify(service.update_status(loan_id, status))


if __name__ == "__main__":
    app.run(port=5000)
